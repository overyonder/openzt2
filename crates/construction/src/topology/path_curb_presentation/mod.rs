use super::path_curb_patterns::curb_pair;
use super::path_placement_highlight_presentation::PathPlacementHighlightAssets;
use bevy::platform::collections::HashMap;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
use openzt2_game_assets::world_definitions::path_placement_highlight_asset::PathPlacementHighlightAsset;
use openzt2_game_data::AssetId;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabPresentation;

use super::path_presentation_neighborhood::{
    cells_for_region, curb_neighborhood_radius, terrain_region,
};
use openzt2_zoo_state::topology::fence_segment_prefab_selection::select_authored_fence_segment_prefab_for_adjacent_topology;
use openzt2_zoo_state::topology::fence_segment_prefab_selection::select_reciprocal_fence_curve_partners;
use openzt2_zoo_state::topology::ground_path_boundary_queries::{
    ground_path_tile_triangles, ground_path_triangle_curb_boundary,
};
use openzt2_zoo_state::topology::ground_path_layout_calculation::calculate_snapped_ground_path_layout;
use openzt2_zoo_state::topology::topology_edit_types::PathConnectivityChanged;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::topology::topology_graph_types::{TopologyGrid, TopologyIndex};

#[derive(Clone, Copy)]
struct GroundPathMacroCellCurbPresentationRecord {
    owner: Entity,
    curb: AssetId,
    cell: IVec3,
    centre_cell: IVec3,
    width_cells: i32,
}

#[derive(Clone, Copy)]
struct GroundPathConnectedCurbBoundarySegment {
    owner: Entity,
    curb: AssetId,
    first_cell: IVec3,
    second_cell: IVec3,
}

#[derive(Component)]
pub(super) struct PathCurbRoot;

#[derive(Component)]
pub(super) struct PathCurbSegmentPresentation;

#[derive(Component)]
pub(super) struct EmptyPathCurbPresentation;

#[derive(Message)]
pub(super) struct DiscardedPathCurbSegment(Entity);

pub(super) fn notify_discarded_path_curb_segment(
    discarded: On<Discard<PathCurbSegmentPresentation>>,
    parents: Query<&ChildOf>,
    mut discarded_segments: MessageWriter<DiscardedPathCurbSegment>,
) {
    if let Ok(parent) = parents.get(discarded.entity) {
        discarded_segments.write(DiscardedPathCurbSegment(parent.parent()));
    }
}

/// Invalidates curb hierarchies whose exposed-edge result can have changed.
///
/// Include paths affected by neighbouring triangles, reciprocal curves and
/// the terrain sampled beneath each segment.
pub(super) fn invalidate_path_curb_presentations_after_topology_or_terrain_changes(
    mut commands: Commands,
    mut changed: MessageReader<PathConnectivityChanged>,
    mut terrain_changed: MessageReader<TerrainChanged>,
    terrain_chunks: Query<&TerrainChunk>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    paths: Query<(Entity, Option<&Children>), With<PathTile>>,
    curb_roots: Query<(), With<PathCurbRoot>>,
    mut discarded_segments: MessageReader<DiscardedPathCurbSegment>,
    (pattern_asset, mut pattern_events): (
        Res<PathPlacementHighlightAssets>,
        MessageReader<AssetEvent<PathPlacementHighlightAsset>>,
    ),
    index: Single<&TopologyIndex>,
    grid: Single<&TopologyGrid>,
) {
    for segment in discarded_segments.read() {
        if curb_roots.contains(segment.0) {
            commands.entity(segment.0).try_despawn();
        }
    }
    let index = index.into_inner();
    let grid = grid.into_inner();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let mut affected = bevy::ecs::entity::EntityHashSet::new();
    if pattern_events
        .read()
        .filter(|event| {
            event.is_modified(pattern_asset.rules.id())
                || event.is_removed(pattern_asset.rules.id())
        })
        .count()
        != 0
    {
        affected.extend(paths.iter().map(|(entity, _)| entity));
    }
    let radius = IVec2::splat(curb_neighborhood_radius(definitions));
    for change in changed.read() {
        let cell = change.0.xy();
        affected.extend(index.paths_within(cell - radius, cell + radius));
    }
    let reach = f32::from(definitions.maximum_ground_path_width_cm()) * 0.005;
    for (min, max) in terrain_changed
        .read()
        .filter_map(|change| terrain_region(change, &terrain_chunks))
    {
        let (min, max) = cells_for_region(*grid, min, max, reach);
        affected.extend(index.paths_within(min, max));
    }
    for entity in affected {
        if let Ok((_, children)) = paths.get(entity) {
            for child in children.into_iter().flat_map(|children| children.iter()) {
                if curb_roots.contains(child) {
                    commands.entity(child).try_despawn();
                }
            }
            commands
                .entity(entity)
                .remove::<EmptyPathCurbPresentation>();
        }
    }
}

/// Projects connected curb segments around the boundary of the current path
/// occupancy. Segment endpoints remain topology cells, allowing the existing
/// canonical fence-prefab selector to choose cardinal, diagonal, 90-degree,
/// and 135-degree authored families from each ordered endpoint pair.
#[allow(clippy::too_many_arguments)]
pub(super) fn hydrate_authored_path_curb_prefab_presentations(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    grid: Single<&TopologyGrid>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    terrain_index: Single<&TerrainIndex>,
    index: Single<&TopologyIndex>,
    paths: Query<&PathTile>,
    candidates: Query<
        (Entity, &PathTile, &Transform, Option<&Children>),
        Without<EmptyPathCurbPresentation>,
    >,
    curb_roots: Query<(), With<PathCurbRoot>>,
    (patterns, pattern_asset): (
        Res<Assets<PathPlacementHighlightAsset>>,
        Res<PathPlacementHighlightAssets>,
    ),
) {
    let Some(patterns) = patterns.get(&pattern_asset.rules) else {
        return;
    };
    let grid = grid.into_inner();
    let terrain_index = terrain_index.into_inner();
    let index = index.into_inner();
    let mut dirty_paths = candidates
        .iter()
        .filter(|(_, _, _, children)| {
            !children
                .is_some_and(|children| children.iter().any(|child| curb_roots.contains(child)))
        })
        .peekable();
    if dirty_paths.peek().is_none() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let radius = IVec2::splat(curb_neighborhood_radius(definitions));
    let path_row = |(owner, path): (Entity, &PathTile)| {
        let definition = definitions.find_path(path.definition)?;
        if definition.elevated {
            return None;
        }
        let (centre_cell, width_cells, _) = calculate_snapped_ground_path_layout(
            path.cell,
            definition.width_cm,
            definitions.topology_cell_size_cm(),
        )?;
        Some(GroundPathMacroCellCurbPresentationRecord {
            owner,
            curb: AssetId(definition.curb.0),
            cell: path.cell,
            centre_cell,
            width_cells,
        })
    };
    let ready = dirty_paths
        .filter_map(|(entity, tile, transform, _)| {
            let Some(path) = path_row((entity, tile)).filter(|path| {
                path.curb != AssetId::default() && index.paths.get(&path.cell) == Some(&entity)
            }) else {
                commands.entity(entity).insert(EmptyPathCurbPresentation);
                return None;
            };
            Some((path, transform))
        })
        .collect::<Vec<_>>();
    let neighbourhood: bevy::ecs::entity::EntityHashSet = ready
        .iter()
        .flat_map(|(path, _)| {
            let centre = path.centre_cell.xy();
            index.paths_within(centre - radius, centre + radius)
        })
        .collect();
    let mut neighbours: Vec<_> = neighbourhood
        .into_iter()
        .filter_map(|entity| path_row((entity, paths.get(entity).ok()?)))
        .collect();
    neighbours.sort_unstable_by_key(|row| (row.cell.to_array(), row.owner.to_bits()));
    let triangles: HashSet<_> = neighbours
        .iter()
        .flat_map(|path| ground_path_tile_triangles(path.cell, path.centre_cell, path.width_cells))
        .collect();
    let mut connected_curb_boundary_segments = neighbours
        .iter()
        .flat_map(|path| {
            ground_path_tile_triangles(path.cell, path.centre_cell, path.width_cells)
                .flat_map(|triangle| {
                    ground_path_triangle_curb_boundary(&triangles, path.centre_cell, triangle)
                })
                .map(
                    |(first_cell, second_cell)| GroundPathConnectedCurbBoundarySegment {
                        owner: path.owner,
                        curb: path.curb,
                        first_cell,
                        second_cell,
                    },
                )
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut by_edge = HashMap::new();
    let key = |curb, [a, b]: [IVec3; 2]| {
        (
            curb,
            openzt2_zoo_state::topology::topology_graph_types::EdgeKey::new(a, b),
        )
    };
    for (index, segment) in connected_curb_boundary_segments.iter().enumerate() {
        by_edge.insert(
            key(segment.curb, [segment.first_cell, segment.second_cell]),
            index,
        );
    }
    let mut authored_partners = HashMap::new();
    let mut matched_centres = HashSet::new();
    for owner in &neighbours {
        if owner.curb == AssetId::default()
            || !matched_centres.insert((owner.centre_cell, owner.width_cells, owner.curb))
        {
            continue;
        }
        for direction in [0, 2, 4, 6] {
            let Some(edges) = curb_pair(
                patterns,
                owner.centre_cell,
                direction,
                owner.width_cells / 2,
                |cell| triangles.contains(&cell),
            ) else {
                continue;
            };
            let [a, b] = edges.map(|edge| {
                *by_edge.entry(key(owner.curb, edge)).or_insert_with(|| {
                    let index = connected_curb_boundary_segments.len();
                    connected_curb_boundary_segments.push(GroundPathConnectedCurbBoundarySegment {
                        owner: owner.owner,
                        curb: owner.curb,
                        first_cell: edge[0],
                        second_cell: edge[1],
                    });
                    index
                })
            });
            let [first, second] = edges;
            if let Some(corner) = first.into_iter().find(|point| second.contains(point)) {
                let outside_a = if first[0] == corner {
                    first[1]
                } else {
                    first[0]
                };
                let outside_b = if second[0] == corner {
                    second[1]
                } else {
                    second[0]
                };
                authored_partners.insert(a, (outside_a, corner, outside_b));
                authored_partners.insert(b, (outside_b, corner, outside_a));
            }
        }
    }
    let curve_edges = connected_curb_boundary_segments
        .iter()
        .map(|segment| (segment.curb, segment.first_cell, segment.second_cell))
        .collect::<Vec<_>>();
    let curve_partners = select_reciprocal_fence_curve_partners(&curve_edges);
    let mut segments_by_owner = bevy::ecs::entity::EntityHashMap::<Vec<_>>::default();
    for (index, segment) in connected_curb_boundary_segments.into_iter().enumerate() {
        let Some(curb) = definitions.find_fence(segment.curb) else {
            continue;
        };
        let partner = authored_partners
            .get(&index)
            .copied()
            .or(curve_partners[index]);
        let (first, second, next) = partner.map_or(
            (segment.first_cell, segment.second_cell, None),
            |(first, second, next)| (first, second, Some(next)),
        );
        let prefab =
            select_authored_fence_segment_prefab_for_adjacent_topology(curb, first, second, next);
        if prefab.asset != AssetId::default() {
            segments_by_owner.entry(segment.owner).or_default().push((
                first,
                second,
                prefab.asset,
                prefab.mirror_source_y,
            ));
        }
    }
    for (path, transform) in ready {
        let entity = path.owner;
        let selected_segments = segments_by_owner.remove(&entity).unwrap_or_default();
        // Prefab presentation keeps strong handles throughout
        // asynchronous loading and renders each segment once it is ready.
        // Looking up Assets here and dropping an unready handle cancels the
        // demand load, repeating the complete boundary scan every frame.
        let Some(ready_segments) = selected_segments
            .iter()
            .map(|&(first, second, asset, mirror_source_y)| {
                let handle = definitions.scene(asset)?;
                Some((first, second, mirror_source_y, handle))
            })
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let mut root = None;
        for (first, second, mirror_source_y, prefab) in ready_segments {
            let root = *root.get_or_insert_with(|| {
                commands
                    .spawn((
                        Name::new("path curbs"),
                        Transform::IDENTITY,
                        Visibility::Inherited,
                        ChildOf(entity),
                        PathCurbRoot,
                    ))
                    .id()
            });
            let first_world = grid.cell_translation(first);
            let second_world = grid.cell_translation(second);
            let local_y = terrain_chunk_at(&terrain_index, first_world.xz())
                .and_then(|chunk| terrain_chunks.get(chunk).ok())
                .and_then(|(chunk, edited)| {
                    let asset = terrain_assets.get(&chunk.asset)?;
                    sample_terrain(chunk, asset, edited, first_world.xz())
                        .map(|point| point.height_m - transform.translation.y)
                })
                .unwrap_or_default();
            let direction = second_world - first_world;
            commands.spawn((
                Transform::from_translation(Vec3::new(
                    first_world.x - transform.translation.xz().x,
                    local_y,
                    first_world.z - transform.translation.xz().y,
                ))
                .with_rotation(Quat::from_rotation_y((-direction.z).atan2(direction.x)))
                .with_scale(Vec3::new(
                    1.0,
                    1.0,
                    if mirror_source_y { -1.0 } else { 1.0 },
                )),
                Visibility::Inherited,
                ChildOf(root),
                PathCurbSegmentPresentation,
                PrefabPresentation::new(prefab),
            ));
        }
        if root.is_none() {
            commands.entity(entity).insert(EmptyPathCurbPresentation);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boundary(triangles: &[IVec3]) -> HashSet<(IVec3, IVec3)> {
        let set = triangles.iter().copied().collect::<HashSet<_>>();
        triangles
            .iter()
            .flat_map(|triangle| {
                ground_path_triangle_curb_boundary(&set, IVec3::new(2, 2, 0), *triangle)
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn whole_square_is_bounded_counter_clockwise() {
        let c = IVec3::new(2, 2, 0);
        let all = [IVec3::Y, IVec3::X, -IVec3::Y, -IVec3::X].map(|side| c + side);
        let sw = IVec3::new(0, 0, 0);
        let se = IVec3::new(4, 0, 0);
        let ne = IVec3::new(4, 4, 0);
        let nw = IVec3::new(0, 4, 0);
        assert_eq!(
            boundary(&all),
            [(sw, se), (se, ne), (ne, nw), (nw, sw)]
                .into_iter()
                .collect()
        );
    }

    #[test]
    fn north_and_east_triangles_are_cut_by_one_diagonal() {
        let c = IVec3::new(2, 2, 0);
        let nw = IVec3::new(0, 4, 0);
        let ne = IVec3::new(4, 4, 0);
        let se = IVec3::new(4, 0, 0);
        assert_eq!(
            boundary(&[c + IVec3::Y, c + IVec3::X]),
            [(ne, nw), (se, ne), (nw, se)].into_iter().collect()
        );
    }
}
