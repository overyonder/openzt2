use bevy::{
    ecs::entity::EntityHashSet,
    prelude::*,
    tasks::{ComputeTaskPool, TaskPool},
};

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::topology::topology_edit_types::TopologyChanged;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::habitat_fence_inputs::{HabitatBoundaryRaster, HabitatFenceLattice};
use crate::habitat::habitat_region_derivation::{
    connected_cardinal_fence_components, derive_enclosed_habitats,
};
use crate::habitat::habitat_terrain_summary_calculation::calculate_habitat_terrain_summary_from_topology_cells;
use crate::habitat::habitat_terrain_summary_refresh::derive_dominant_habitat_climate;
use crate::habitat::habitat_terrain_summary_refresh::read_terrain_fact_for_topology_cell;
use openzt2_zoo_state::habitat::habitat_topology_cell_queries::sorted_topology_cells_contain_cell;
use openzt2_zoo_state::habitat::habitat_types::HabitatChanged;
use openzt2_zoo_state::habitat::habitat_types::HabitatClimate;
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatSummary;
use openzt2_zoo_state::habitat::habitat_types::RebuildHabitatRegionsWithinTopologyBounds;
use openzt2_zoo_state::habitat::habitat_types::{Habitat, HabitatBoundaryFence};

pub(super) fn link_changed_habitat_boundaries(
    mut commands: Commands,
    habitats: Query<(Entity, &HabitatRegion, Option<&Children>), Changed<HabitatRegion>>,
    edges: Query<&HabitatBoundaryFence>,
) {
    for (habitat, region, children) in &habitats {
        let mut desired: bevy::ecs::entity::EntityHashSet =
            region.boundary_fence_entities.iter().copied().collect();
        for edge in children.into_iter().flat_map(|children| children.iter()) {
            if let Ok(fence) = edges.get(edge) {
                if !desired.remove(&fence.0) {
                    commands.entity(edge).try_despawn();
                }
            }
        }
        for fence in desired {
            commands.spawn((ChildOf(habitat), HabitatBoundaryFence(fence)));
        }
    }
}

#[derive(Component)]
pub(super) struct HabitatTopologyProjected;

/// Seeds the first regional rebuild from topology already hydrated with the map.
pub(super) fn request_initial_habitat_region_rebuild_from_loaded_topology(
    topology_nodes: Query<&TopologyNode>,
    published: Query<(), (With<WorldRoot>, With<HabitatTopologyProjected>)>,
    mut habitat_rebuild_requests: MessageWriter<RebuildHabitatRegionsWithinTopologyBounds>,
) {
    if !published.is_empty() {
        return;
    }
    let Some(topology_bounds) = calculate_topology_cell_bounds(topology_nodes.iter()) else {
        return;
    };
    habitat_rebuild_requests.write(RebuildHabitatRegionsWithinTopologyBounds(topology_bounds));
}

pub(super) fn request_habitat_region_rebuild_for_added_topology_nodes(
    added_topology_nodes: Query<&TopologyNode, Added<TopologyNode>>,
    mut habitat_rebuild_requests: MessageWriter<RebuildHabitatRegionsWithinTopologyBounds>,
) {
    let Some(topology_bounds) = calculate_topology_cell_bounds(added_topology_nodes.iter()) else {
        return;
    };
    habitat_rebuild_requests.write(RebuildHabitatRegionsWithinTopologyBounds(topology_bounds));
}

pub(super) fn request_habitat_region_rebuilds_after_topology_changes(
    mut topology_changes: MessageReader<TopologyChanged>,
    mut habitat_rebuild_requests: MessageWriter<RebuildHabitatRegionsWithinTopologyBounds>,
) {
    for topology_change in topology_changes.read() {
        habitat_rebuild_requests.write(RebuildHabitatRegionsWithinTopologyBounds(
            topology_change.bounds,
        ));
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn rebuild_changed_habitat_regions_from_fence_topology_and_terrain(
    mut commands: Commands,
    mut habitat_rebuild_requests: MessageReader<RebuildHabitatRegionsWithinTopologyBounds>,
    mut habitat_index: ResMut<HabitatIndex>,
    terrain_index: Res<TerrainIndex>,
    topology_grid: Res<TopologyGrid>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    fence_lattice: Res<HabitatFenceLattice>,
    rasters: Query<&HabitatBoundaryRaster>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    world_roots: Query<Entity, With<WorldRoot>>,
    mut existing_habitats: Query<
        (
            Entity,
            &mut HabitatRegion,
            &mut HabitatSummary,
            Option<&mut HabitatClimate>,
            Option<&mut WorldMember>,
        ),
        With<Habitat>,
    >,
    mut habitat_changes: MessageWriter<HabitatChanged>,
) {
    let Some(changed_topology_bounds) =
        merge_requested_habitat_region_rebuild_bounds(&mut habitat_rebuild_requests)
    else {
        return;
    };
    debug!(
        ?changed_topology_bounds,
        roots = world_roots.iter().count(),
        "requested habitat rebuild"
    );
    let Ok(world_root) = world_roots.single() else {
        return;
    };
    let topology_cell_size_metres = topology_grid.spacing_m;
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };

    let mut affected = EntityHashSet::default();
    let mut visited_fences = EntityHashSet::default();
    let mut loaded_fences = Vec::new();
    let mut bounds = changed_topology_bounds;
    // Expand using existing habitat interiors and connected fence footprints.
    // No fence geometry is derived while finding the affected components.
    loop {
        let before = bounds;
        let nearby = IRect::from_corners(bounds.min - IVec2::ONE, bounds.max + IVec2::ONE);
        for y in nearby.min.y..=nearby.max.y {
            for x in nearby.min.x..=nearby.max.x {
                let Some(habitat) = habitat_index.habitat_for_topology_cell(IVec2::new(x, y))
                else {
                    continue;
                };
                if affected.insert(habitat) {
                    if let Ok((_, region, ..)) = existing_habitats.get(habitat) {
                        if let Some(footprint) = calculate_topology_cell_bounds_of_cells(
                            region.topology_cells.iter().copied(),
                        ) {
                            bounds = bounds.union(IRect::from_corners(
                                footprint.min,
                                footprint.max + IVec2::ONE,
                            ));
                        }
                    }
                }
            }
        }
        let rectangle = bounds;
        let mut frontier = Vec::new();
        for y in rectangle.min.y..=rectangle.max.y {
            for x in rectangle.min.x..=rectangle.max.x {
                frontier.extend(
                    fence_lattice
                        .at(IVec2::new(x, y))
                        .filter(|fence| visited_fences.insert(*fence)),
                );
            }
        }
        while let Some(fence) = frontier.pop() {
            let Ok(raster) = rasters.get(fence) else {
                continue;
            };
            for piece in &raster.0 {
                for node in [piece.first_topology_node, piece.second_topology_node] {
                    bounds = bounds.union_point(node);
                    frontier.extend(
                        fence_lattice
                            .at(node)
                            .filter(|fence| visited_fences.insert(*fence)),
                    );
                }
                loaded_fences.push(*piece);
            }
        }
        if bounds == before {
            break;
        }
    }
    let components = connected_cardinal_fence_components(&loaded_fences);
    // Nested or overlapping boundary components must share an outside flood.
    // Disjoint bounds can be solved independently without altering cell order.
    let mut groups: Vec<(IRect, Vec<_>)> = Vec::new();
    for component in components {
        let mut bounds = component.bounds;
        let mut segments = component.segments;
        let mut cursor = 0;
        while cursor < groups.len() {
            let (other, _) = &groups[cursor];
            if bounds.min.cmple(other.max).all() && bounds.max.cmpge(other.min).all() {
                let (other, moved) = groups.remove(cursor);
                bounds = bounds.union(other);
                segments.extend(moved);
                cursor = 0;
            } else {
                cursor += 1;
            }
        }
        groups.push((bounds, segments));
    }
    let mut derived_regions: Vec<_> = ComputeTaskPool::get_or_init(TaskPool::default)
        .scope(|scope| {
            for (_, segments) in groups {
                scope.spawn(async move { derive_enclosed_habitats(&segments) });
            }
        })
        .into_iter()
        .flatten()
        .collect();
    derived_regions
        .sort_unstable_by_key(|region| region.topology_cells.first().map(|cell| (cell.x, cell.y)));
    let mut affected_existing_habitats = affected
        .into_iter()
        .filter_map(|entity| {
            let (_, region, ..) = existing_habitats.get(entity).ok()?;
            Some((entity, region.topology_cells.clone()))
        })
        .collect::<Vec<_>>();
    commands
        .entity(world_root)
        .insert_if_new(HabitatTopologyProjected);
    affected_existing_habitats.sort_unstable_by_key(|(entity, _)| entity.to_bits());
    let mut reused_existing_habitats = vec![false; affected_existing_habitats.len()];

    for (habitat_entity, topology_cells) in &affected_existing_habitats {
        for topology_cell in topology_cells.iter() {
            habitat_index
                .remove_habitat_for_topology_cell_if_it_matches(*topology_cell, *habitat_entity);
        }
    }

    for derived_region in derived_regions {
        let habitat_entity = find_best_reusable_habitat_region_index(
            &derived_region.topology_cells,
            &affected_existing_habitats,
            &reused_existing_habitats,
        )
        .map(|reusable_habitat_index| {
            reused_existing_habitats[reusable_habitat_index] = true;
            affected_existing_habitats[reusable_habitat_index].0
        })
        .unwrap_or_else(|| commands.spawn_empty().id());
        let topology_cell_area_square_metres =
            topology_cell_size_metres * topology_cell_size_metres;
        let mut read_terrain_cell_fact = |topology_cell| {
            read_terrain_fact_for_topology_cell(
                topology_cell,
                *topology_grid,
                &terrain_index,
                &terrain_assets,
                &terrain_chunks,
            )
        };
        let habitat_summary = calculate_habitat_terrain_summary_from_topology_cells(
            &derived_region.topology_cells,
            topology_cell_area_square_metres,
            derived_region.boundary_is_breached,
            &mut read_terrain_cell_fact,
        );
        debug!(
            ?habitat_entity,
            ?habitat_summary,
            "derived habitat terrain summary"
        );
        let habitat_region = HabitatRegion {
            area_square_metres: topology_cell_area_square_metres
                * derived_region.topology_cells.len() as f32,
            topology_cells: derived_region.topology_cells.into_boxed_slice(),
            boundary_fence_entities: derived_region.boundary_fence_entities.into_boxed_slice(),
        };
        for topology_cell in habitat_region.topology_cells.iter().copied() {
            habitat_index.record_habitat_for_topology_cell(topology_cell, habitat_entity);
        }
        let habitat_climate = derive_dominant_habitat_climate(&habitat_summary, world_definitions);
        let member = WorldMember { root: world_root };
        if let Ok((_, mut region, mut summary, climate, world)) =
            existing_habitats.get_mut(habitat_entity)
        {
            region.set_if_neq(habitat_region);
            summary.set_if_neq(habitat_summary);
            match (climate, habitat_climate) {
                (Some(mut current), Some(next)) => {
                    current.set_if_neq(next);
                }
                (None, Some(next)) => {
                    commands.entity(habitat_entity).insert(next);
                }
                (Some(_), None) => {
                    commands.entity(habitat_entity).remove::<HabitatClimate>();
                }
                (None, None) => {}
            }
            if let Some(mut world) = world {
                world.set_if_neq(member);
            } else {
                commands.entity(habitat_entity).insert(member);
            }
        } else {
            let mut spawned = commands.entity(habitat_entity);
            spawned.insert((Habitat, habitat_region, habitat_summary, member));
            if let Some(climate) = habitat_climate {
                spawned.insert(climate);
            }
        }
        habitat_changes.write(HabitatChanged {
            habitat_entity,
            changed_topology_bounds,
        });
    }

    for (existing_habitat_index, (habitat_entity, _)) in
        affected_existing_habitats.iter().enumerate()
    {
        if !reused_existing_habitats[existing_habitat_index] {
            habitat_changes.write(HabitatChanged {
                habitat_entity: *habitat_entity,
                changed_topology_bounds,
            });
            commands.entity(*habitat_entity).despawn();
        }
    }
}

fn calculate_topology_cell_bounds<'a>(
    topology_nodes: impl Iterator<Item = &'a TopologyNode>,
) -> Option<IRect> {
    calculate_topology_cell_bounds_of_cells(
        topology_nodes.map(|topology_node| topology_node.cell.truncate()),
    )
}

fn calculate_topology_cell_bounds_of_cells(
    topology_cells: impl Iterator<Item = IVec2>,
) -> Option<IRect> {
    topology_cells.fold(None, |bounds: Option<IRect>, topology_cell| {
        Some(bounds.map_or_else(
            || IRect::from_corners(topology_cell, topology_cell),
            |bounds| {
                IRect::from_corners(bounds.min.min(topology_cell), bounds.max.max(topology_cell))
            },
        ))
    })
}

fn merge_requested_habitat_region_rebuild_bounds(
    habitat_rebuild_requests: &mut MessageReader<RebuildHabitatRegionsWithinTopologyBounds>,
) -> Option<IRect> {
    habitat_rebuild_requests
        .read()
        .fold(None, |merged_bounds, request| {
            Some(merged_bounds.map_or(request.0, |current: IRect| {
                IRect::from_corners(
                    current.min.min(request.0.min),
                    current.max.max(request.0.max),
                )
            }))
        })
}

fn find_best_reusable_habitat_region_index(
    derived_region_cells: &[IVec2],
    affected_existing_habitats: &[(Entity, Box<[IVec2]>)],
    reused_existing_habitats: &[bool],
) -> Option<usize> {
    affected_existing_habitats
        .iter()
        .enumerate()
        .filter(|(habitat_index, _)| !reused_existing_habitats[*habitat_index])
        .map(|(habitat_index, (_, existing_region_cells))| {
            let overlapping_cell_count = derived_region_cells
                .iter()
                .filter(|cell| sorted_topology_cells_contain_cell(existing_region_cells, **cell))
                .count();
            (habitat_index, overlapping_cell_count)
        })
        .filter(|(_, overlapping_cell_count)| *overlapping_cell_count > 0)
        .max_by_key(|(habitat_index, overlapping_cell_count)| {
            (*overlapping_cell_count, std::cmp::Reverse(*habitat_index))
        })
        .map(|(habitat_index, _)| habitat_index)
}
