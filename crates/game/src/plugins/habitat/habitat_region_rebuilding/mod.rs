use bevy::prelude::*;
use openzt2_game_data::world_definitions::fences_and_gates::FenceTraversalBlockingFlags;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::topology::gate_operation_types::Gate;
use crate::plugins::topology::topology_edit_types::TopologyChanged;
use crate::plugins::topology::topology_graph_types::FenceEdge;
use crate::plugins::topology::topology_graph_types::TopologyGrid;
use crate::plugins::topology::topology_graph_types::TopologyNode;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    habitat_region_derivation::{
        derive_enclosed_habitat_regions_from_fence_segments, HabitatFenceSegment,
    },
    habitat_terrain_summary_calculation::calculate_habitat_terrain_summary_from_topology_cells,
    habitat_terrain_summary_refresh::{
        derive_dominant_habitat_climate, read_terrain_fact_for_topology_cell,
    },
    habitat_topology_cell_queries::{
        sorted_topology_cells_contain_cell, topology_cells_overlap_inclusive_bounds,
    },
    habitat_types::{
        Habitat, HabitatChanged, HabitatClimate, HabitatIndex, HabitatRegion, HabitatSummary,
        RebuildHabitatRegionsWithinTopologyBounds,
    },
};

/// Seeds the first regional rebuild from topology already hydrated with the map.
pub(super) fn request_initial_habitat_region_rebuild_from_loaded_topology(
    topology_nodes: Query<&TopologyNode>,
    mut habitat_rebuild_requests: MessageWriter<RebuildHabitatRegionsWithinTopologyBounds>,
) {
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
    fence_edges: Query<(Entity, &FenceEdge, Option<&Gate>)>,
    topology_nodes: Query<&TopologyNode>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    world_roots: Query<Entity, With<WorldRoot>>,
    existing_habitats: Query<(Entity, &HabitatRegion, &HabitatSummary), With<Habitat>>,
    mut habitat_changes: MessageWriter<HabitatChanged>,
) {
    let Some(changed_topology_bounds) =
        merge_requested_habitat_region_rebuild_bounds(&mut habitat_rebuild_requests)
    else {
        return;
    };
    let Ok(world_root) = world_roots.single() else {
        return;
    };
    let topology_cell_size_metres = topology_grid.spacing_m;
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };

    let mut fence_segments = Vec::with_capacity(fence_edges.iter().len());
    for (fence_entity, fence_edge, gate) in &fence_edges {
        let (Ok(first_node), Ok(second_node)) = (
            topology_nodes.get(fence_edge.a),
            topology_nodes.get(fence_edge.b),
        ) else {
            continue;
        };
        let definition = world_definitions.find_fence(fence_edge.definition);
        let blocks_animals = definition.is_some_and(|definition| {
            definition
                .blocks
                .contains_all(FenceTraversalBlockingFlags::ANIMAL)
        });
        if !blocks_animals || gate.is_some_and(|gate| gate.open) {
            info!(target: "openzt2_gameplay_journey", ?fence_entity,
                definition = ?fence_edge.definition,
                blocks = ?definition.map(|definition| definition.blocks),
                ?gate, "habitat boundary fence is breached");
        }
        fence_segments.push(HabitatFenceSegment {
            fence_entity,
            first_topology_node: first_node.cell.truncate(),
            second_topology_node: second_node.cell.truncate(),
            boundary_is_breached: !blocks_animals || gate.is_some_and(|gate| gate.open),
        });
    }
    let derived_regions = derive_enclosed_habitat_regions_from_fence_segments(
        &fence_segments,
        changed_topology_bounds,
    );

    let affected_topology_bounds = IRect::from_corners(
        changed_topology_bounds.min - IVec2::ONE,
        changed_topology_bounds.max + IVec2::ONE,
    );
    let mut affected_existing_habitats = Vec::new();
    for (habitat_entity, habitat_region, _) in &existing_habitats {
        if topology_cells_overlap_inclusive_bounds(
            &habitat_region.topology_cells,
            affected_topology_bounds,
        ) {
            affected_existing_habitats
                .push((habitat_entity, habitat_region.topology_cells.as_ref()));
        }
    }
    affected_existing_habitats.sort_unstable_by_key(|(entity, _)| entity.to_bits());
    let mut reused_existing_habitats = vec![false; affected_existing_habitats.len()];

    for (habitat_entity, topology_cells) in &affected_existing_habitats {
        for topology_cell in *topology_cells {
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
        commands.entity(habitat_entity).insert((
            Habitat,
            habitat_region,
            habitat_summary,
            WorldMember { root: world_root },
        ));
        if let Some(habitat_climate) = habitat_climate {
            commands.entity(habitat_entity).insert(habitat_climate);
        } else {
            commands.entity(habitat_entity).remove::<HabitatClimate>();
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
    topology_nodes
        .map(|topology_node| topology_node.cell.truncate())
        .fold(None, |bounds: Option<IRect>, topology_cell| {
            Some(bounds.map_or_else(
                || IRect::from_corners(topology_cell, topology_cell),
                |bounds| {
                    IRect::from_corners(
                        bounds.min.min(topology_cell),
                        bounds.max.max(topology_cell),
                    )
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
    affected_existing_habitats: &[(Entity, &[IVec2])],
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
