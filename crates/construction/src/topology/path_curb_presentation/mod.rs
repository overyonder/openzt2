use bevy::platform::collections::HashMap;
use bevy::platform::collections::HashSet;
use bevy::prelude::*;
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

use openzt2_zoo_state::topology::fence_segment_prefab_selection::select_authored_fence_segment_prefab_for_adjacent_topology;
use openzt2_zoo_state::topology::fence_segment_prefab_selection::select_reciprocal_fence_curve_partners;
use openzt2_zoo_state::topology::ground_path_boundary_queries::{
    ground_path_tile_triangles, ground_path_triangle_curb_boundary,
};
use openzt2_zoo_state::topology::ground_path_layout_calculation::calculate_snapped_ground_path_layout;
use openzt2_zoo_state::topology::topology_edit_types::TopologyChanged;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

#[derive(Clone, Copy)]
struct GroundPathMacroCellCurbPresentationRecord {
    owner: Entity,
    definition: AssetId,
    curb: AssetId,
    cell: IVec3,
    centre_cell: IVec3,
    width_cells: i32,
    tile_centre: Vec2,
}

#[derive(Clone, Copy)]
struct GroundPathConnectedCurbBoundarySegment {
    owner: Entity,
    curb: AssetId,
    first_cell: IVec3,
    second_cell: IVec3,
}

/// Rendering state for the curb hierarchy projected from the current path
/// neighborhood. Connectivity remains owned by PathTile and TopologyIndex.
#[derive(Component)]
pub struct PathCurbsHydrated {
    root: Option<Entity>,
    segments: Vec<(IVec3, IVec3, AssetId, bool)>,
}

impl PathCurbsHydrated {
    pub const fn presentation_root(&self) -> Option<Entity> {
        self.root
    }

    pub const fn has_projected_segments(&self) -> bool {
        self.root.is_some()
    }
}

/// Invalidates curb hierarchies whose exposed-edge result can have changed.
///
/// A path insertion or removal changes the presentation of that cell and its
/// four immediate neighbors. The topology edit message already owns that
/// changed region, so curb projection remains reactive without scanning and
/// rebuilding every path every frame.
pub(crate) fn invalidate_path_curb_presentations_after_topology_or_terrain_changes(
    mut commands: Commands,
    mut changed: MessageReader<TopologyChanged>,
    mut terrain_changed: MessageReader<TerrainChanged>,
    terrain_chunks: Query<&TerrainChunk>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    paths: Query<(Entity, &PathTile, &Transform, &PathCurbsHydrated)>,
) {
    if changed.is_empty() && terrain_changed.is_empty() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let mut invalidated = HashSet::new();
    let radius = definitions
        .paths()
        .filter_map({
            let cell_size_cm = definitions.topology_cell_size_cm();
            move |path| {
                calculate_snapped_ground_path_layout(IVec3::ZERO, path.width_cm, cell_size_cm)
                    .map(|(_, _, placement_step)| placement_step)
            }
        })
        .max()
        .unwrap_or(1);
    for change in changed.read() {
        let min = change.bounds.min - IVec2::splat(radius);
        let max = change.bounds.max + IVec2::splat(radius);
        for (entity, path, _, _) in &paths {
            let cell = path.cell.xy();
            if cell.x < min.x || cell.y < min.y || cell.x > max.x || cell.y > max.y {
                continue;
            }
            invalidated.insert(entity);
        }
    }
    let changed_regions = terrain_changed
        .read()
        .filter_map(|change| {
            let chunk = terrain_chunks.get(change.chunk).ok()?;
            Some((
                chunk.origin + change.min.as_vec2() * chunk.spacing_m,
                chunk.origin + change.max.as_vec2() * chunk.spacing_m,
            ))
        })
        .collect::<Vec<_>>();
    for (entity, path, transform, _presentation) in &paths {
        let Some(half_width_m) = (|| {
            let definition = definitions.find_path(path.definition)?;
            (!definition.elevated).then(|| f32::from(definition.width_cm) * 0.005)
        })() else {
            continue;
        };
        let centre = transform.translation.xz();
        if changed_regions.iter().any(|(min, max)| {
            let extent = Vec2::splat(half_width_m);
            centre.cmpge(*min - extent).all() && centre.cmple(*max + extent).all()
        }) {
            invalidated.insert(entity);
        }
    }
    for (entity, _, _, presentation) in &paths {
        if invalidated.contains(&entity) {
            if let Some(root) = presentation.root {
                commands.entity(root).despawn();
            }
            commands.entity(entity).remove::<PathCurbsHydrated>();
        }
    }
}

/// Projects connected curb segments around the boundary of the current path
/// occupancy. Segment endpoints remain topology cells, allowing the existing
/// canonical fence-prefab selector to choose cardinal, diagonal, 90-degree,
/// and 135-degree authored families from each ordered endpoint pair.
#[allow(clippy::too_many_arguments)]
pub(crate) fn hydrate_authored_path_curb_prefab_presentations(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    grid: Res<TopologyGrid>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    terrain_index: Res<TerrainIndex>,
    paths: Query<(Entity, &PathTile, &Transform, Option<&PathCurbsHydrated>)>,
    paths_awaiting_curb_hydration: Query<(), (With<PathTile>, Without<PathCurbsHydrated>)>,
    mut topology_changed: MessageReader<TopologyChanged>,
) {
    let boundary_changed = topology_changed.read().count() != 0;
    if paths_awaiting_curb_hydration.is_empty() && !boundary_changed {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let path_rows = paths
        .iter()
        .filter_map(|(entity, path, transform, _)| {
            (|| {
                let definition = definitions.find_path(path.definition)?;
                if definition.elevated {
                    return None;
                }
                let width_cm = definition.width_cm;
                let cell_size_cm = definitions.topology_cell_size_cm();
                let (centre_cell, width_cells, _) =
                    calculate_snapped_ground_path_layout(path.cell, width_cm, cell_size_cm)?;
                Some(GroundPathMacroCellCurbPresentationRecord {
                    owner: entity,
                    definition: path.definition,
                    curb: AssetId(definition.curb.0),
                    cell: path.cell,
                    centre_cell,
                    width_cells,
                    tile_centre: transform.translation.xz(),
                })
            })()
        })
        .collect::<Vec<_>>();
    let mut owners = HashMap::<(AssetId, IVec3), Entity>::new();
    let mut triangles = HashSet::<IVec3>::new();
    for path in &path_rows {
        triangles.extend(ground_path_tile_triangles(
            path.cell,
            path.centre_cell,
            path.width_cells,
        ));
        owners
            .entry((path.definition, path.cell))
            .and_modify(|owner| {
                if path.owner.to_bits() < owner.to_bits() {
                    *owner = path.owner;
                }
            })
            .or_insert(path.owner);
    }
    let connected_curb_boundary_segments = path_rows
        .iter()
        .filter(|path| owners.get(&(path.definition, path.cell)) == Some(&path.owner))
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

    let curve_edges = connected_curb_boundary_segments
        .iter()
        .map(|segment| (segment.curb, segment.first_cell, segment.second_cell))
        .collect::<Vec<_>>();
    let curve_partners = select_reciprocal_fence_curve_partners(&curve_edges);

    for (entity, _path, transform, hydrated) in &paths {
        let Some(path) = path_rows
            .iter()
            .find(|candidate| candidate.owner == entity)
            .filter(|path| path.curb != AssetId::default())
            .filter(|path| owners.get(&(path.definition, path.cell)) == Some(&entity))
            .copied()
        else {
            if let Some(root) = hydrated.and_then(|current| current.root) {
                commands.entity(root).despawn();
            }
            commands.entity(entity).insert(PathCurbsHydrated {
                root: None,
                segments: Vec::new(),
            });
            continue;
        };
        let Some(curb_definition) = definitions.find_fence(path.curb) else {
            continue;
        };
        let selected_segments = connected_curb_boundary_segments
            .iter()
            .enumerate()
            .filter(|(_, segment)| segment.owner == entity)
            .map(|(index, segment)| {
                let (first, second, next) = curve_partners[index].map_or(
                    (segment.first_cell, segment.second_cell, None),
                    |(first, second, next)| (first, second, Some(next)),
                );
                let prefab = select_authored_fence_segment_prefab_for_adjacent_topology(
                    curb_definition,
                    first,
                    second,
                    next,
                );
                (first, second, prefab)
            })
            .filter(|(_, _, prefab)| prefab.asset != AssetId::default())
            .map(|(first, second, prefab)| (first, second, prefab.asset, prefab.mirror_source_y))
            .collect::<Vec<_>>();
        // Reciprocal curve pairing also depends on the partner's other end.
        // Compare the selected geometry beyond the immediate edited cells.
        if hydrated.is_some_and(|current| current.segments == selected_segments) {
            continue;
        }
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
        if let Some(root) = hydrated.and_then(|current| current.root) {
            commands.entity(root).despawn();
        }
        let mut root = None;
        for (first, second, mirror_source_y, prefab) in ready_segments {
            let root = *root.get_or_insert_with(|| {
                commands
                    .spawn((
                        Name::new("path curbs"),
                        Transform::IDENTITY,
                        Visibility::Inherited,
                        ChildOf(entity),
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
                    first_world.x - path.tile_centre.x,
                    local_y,
                    first_world.z - path.tile_centre.y,
                ))
                .with_rotation(Quat::from_rotation_y((-direction.z).atan2(direction.x)))
                .with_scale(Vec3::new(
                    1.0,
                    1.0,
                    if mirror_source_y { -1.0 } else { 1.0 },
                )),
                Visibility::Inherited,
                ChildOf(root),
                PrefabPresentation::new(prefab),
            ));
        }
        commands.entity(entity).insert(PathCurbsHydrated {
            root,
            segments: selected_segments,
        });
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
