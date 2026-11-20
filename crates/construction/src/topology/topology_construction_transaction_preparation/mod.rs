use super::topology_construction_preview_types::{
    terrain_change_cursor, FenceDrawPlan, PathDrawPlan, TopologyPlanVersion,
};
use crate::construction::construction_transaction_preparation::SubmittedPreview;
use bevy::prelude::*;
use openzt2_zoo_state::topology::{
    habitat_boundary_queries::HabitatBoundaryQueries,
    topology_graph_types::{FencesEndingHere, FencesStartingHere},
};

use crate::placement::stomped_object_edit_preparation::{
    prepare_placement_stomp_removals, StompablePlacedObject,
};
use openzt2_game_data::world_definitions::object_placement::placement_stomp_rules::PlacementStompAction;
use openzt2_zoo_state::placement::{
    placement_occupancy_index::PlacedObjectFootprintOccupancyIndex,
    placement_transaction_types::{ObjectPlacementEditPrepared, PreparedObjectPlacementEdit, PreparedObjectPlacementMutation},
};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::{Tank, TankGeometry, TankWallTopHeight};
use openzt2_zoo_state::aquatic::tank_wall_ground_sampling::TankWallGroundSampling;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::topology::topology_grid_geometry::calculate_adjacent_topology_grid_cells_between_endpoints;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareConstruction;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use crate::topology::topology_construction_planning::calculate_fence_construction_plan;
use crate::topology::topology_construction_planning::calculate_path_construction_plan;
use crate::topology::topology_construction_planning::calculate_tank_wall_top_for_fence_path;
use crate::topology::topology_construction_planning::sample_terrain_height_at_topology_cell;
use crate::topology::topology_construction_planning::{
    calculate_gate_replacement_plan, fence_definition_is_gate, GateHostEdges,
};
use crate::topology::topology_construction_preview_types::FencePreview;
use crate::topology::topology_construction_preview_types::PathPreview;
use crate::topology::topology_edit_outcome_messages::write_topology_edit_preparation_rejection;
use openzt2_zoo_state::topology::gate_operation_types::Gate;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEdit;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPreparationRejected;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPrepared;
use openzt2_zoo_state::topology::topology_edit_types::TopologySnapshot;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;

fn prepare_fence_construction_snapshots(
    submitted: Option<&FenceDrawPlan>,
    preview: &FencePreview,
    index: &TopologyIndex,
    grid: TopologyGrid,
    definitions: WorldDefinitionsView<'_>,
    persistent: &Query<&PersistentId>,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    ids: &mut PersistentIdAllocator,
    root: Entity,
    joined_tank_water_height: impl Fn(IVec3) -> Option<f32>,
    ground: &TankWallGroundSampling,
    paths: &Query<&PathTile>,
    (hosts, wall_tops): (&GateHostEdges, &Query<&TankWallTopHeight>),
) -> Result<(Vec<TopologySnapshot>, Vec<TopologySnapshot>, Money), PlacementFailure> {
    let (cells, cost, gate, replaced) = if let Some((plan, (cost, gate, replaced))) =
        submitted.and_then(|plan| plan.quote.as_ref().map(|quote| (plan, quote)))
    {
        (plan.cells.clone(), *cost, *gate, replaced.clone())
    } else {
        calculate_fence_construction_plan(
            preview,
            index,
            grid,
            definitions,
            terrain_index,
            terrain_assets,
            terrain_chunks,
            hosts,
        )?
    };
    let id = |entity| {
        persistent
            .get(entity)
            .copied()
            .map_err(|_| PlacementFailure::InvalidTopology)
    };
    // The broken segments the run is built over, removed in the same edit.
    let removed = replaced
        .into_iter()
        .map(|edge| {
            let (fence, ..) = hosts
                .get(edge)
                .map_err(|_| PlacementFailure::InvalidTopology)?;
            Ok(TopologySnapshot::Fence {
                id: id(edge)?,
                definition: fence.definition,
                a: id(fence.a())?,
                b: id(fence.b())?,
                gate: None,
                wall_top: wall_tops.get(edge).ok().copied(),
            })
        })
        .collect::<Result<Vec<_>, PlacementFailure>>()?;
    // New tank walls get the top the original's fence mode gives them.
    let tank_wall_top = if definitions.find_tank(preview.definition).is_some() {
        let (Some(policy), Some(surface)) = (
            definitions.fence_placement_mode_policy(),
            definitions.tank_surface_policy(),
        ) else {
            return Err(PlacementFailure::InvalidTopology);
        };
        let top = calculate_tank_wall_top_for_fence_path(
            preview,
            &cells,
            policy,
            surface.water_height_offset_cm as f32 / 100.0,
            |cell| {
                sample_terrain_height_at_topology_cell(
                    cell,
                    grid,
                    terrain_index,
                    terrain_assets,
                    terrain_chunks,
                )
            },
            joined_tank_water_height,
        )
        .ok_or(PlacementFailure::OutsideMap)?;
        Some(TankWallTopHeight(top))
    } else {
        None
    };
    let existing_ids = cells
        .iter()
        .map(|cell| {
            index.nodes.get(cell).map_or(Ok(None), |entity| {
                persistent
                    .get(*entity)
                    .copied()
                    .map(Some)
                    .map_err(|_| PlacementFailure::InvalidTopology)
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut snapshots = Vec::with_capacity(cells.len() * 2 - 1);
    let mut node_ids = Vec::with_capacity(cells.len());
    for (&cell, existing_id) in cells.iter().zip(existing_ids) {
        // A closed shape returns to its first, still unbuilt node.
        let revisited = node_ids
            .iter()
            .find_map(|(visited, id)| (*visited == cell).then_some(*id));
        if let Some(id) = existing_id.or(revisited) {
            node_ids.push((cell, id));
        } else {
            let id = ids
                .allocate(root)
                .map_err(|_| PlacementFailure::InvalidTopology)?;
            snapshots.push(TopologySnapshot::Node { id, cell });
            node_ids.push((cell, id));
        }
    }
    for pair in node_ids.windows(2) {
        let a = pair[0].1;
        let b = pair[1].1;
        let wall_top = if let Some(top) = tank_wall_top {
            let policy = definitions
                .fence_placement_mode_policy()
                .ok_or(PlacementFailure::InvalidTopology)?;
            let samples =
                calculate_adjacent_topology_grid_cells_between_endpoints(pair[0].0, pair[1].0)?;
            let mut minimum = f32::NEG_INFINITY;
            let mut maximum = f32::INFINITY;
            for cell in &samples {
                minimum = minimum.max(
                    ground
                        .ground_height_at_topology_cell(*cell)
                        .ok_or(PlacementFailure::OutsideMap)?
                        + policy.minimum_tank_wall_height_cm as f32 / 100.0,
                );
                maximum = maximum.min(
                    ground
                        .terrain_units_at_topology_cell(*cell)
                        .ok_or(PlacementFailure::OutsideMap)?
                        .maximum_height_metres
                        + policy.default_tank_wall_height_cm as f32 / 100.0,
                );
            }
            for cell in &samples {
                for path in paths
                    .iter_many(index.paths_within(cell.xy(), cell.xy()))
                    .filter_map(Result::ok)
                {
                    if path.cell.z != 0 {
                        maximum = maximum.min(path.cell.z as f32 - 4.0);
                    }
                }
            }
            Some(TankWallTopHeight(top.0.max(minimum).min(maximum)))
        } else {
            None
        };
        snapshots.push(TopologySnapshot::Fence {
            id: ids
                .allocate(root)
                .map_err(|_| PlacementFailure::InvalidTopology)?,
            definition: preview.definition,
            a,
            b,
            gate,
            wall_top,
        });
    }
    Ok((snapshots, removed, cost))
}

/// Swaps the fence segment under a gate preview for the same segment with a
/// closed gate, so undo restores the plain fence.
fn prepare_gate_replacement_snapshots(
    submitted: Option<&FenceDrawPlan>,
    preview: &FencePreview,
    index: &TopologyIndex,
    definitions: WorldDefinitionsView<'_>,
    hosts: &GateHostEdges,
    wall_tops: &Query<&TankWallTopHeight>,
    persistent: &Query<&PersistentId>,
    ids: &mut PersistentIdAllocator,
    root: Entity,
) -> Result<(TopologySnapshot, TopologySnapshot, Money), PlacementFailure> {
    let (edge, cost) = if let Some((edge, (cost, ..))) =
        submitted.and_then(|plan| plan.replaced.zip(plan.quote.as_ref()))
    {
        (edge, *cost)
    } else {
        calculate_gate_replacement_plan(preview, index, definitions, hosts)?
    };
    let (fence, ..) = hosts
        .get(edge)
        .map_err(|_| PlacementFailure::InvalidTopology)?;
    let id = |entity| {
        persistent
            .get(entity)
            .copied()
            .map_err(|_| PlacementFailure::InvalidTopology)
    };
    let (a, b) = (id(fence.a())?, id(fence.b())?);
    let removed = TopologySnapshot::Fence {
        id: id(edge)?,
        definition: fence.definition,
        a,
        b,
        gate: None,
        wall_top: wall_tops.get(edge).ok().copied(),
    };
    let created = TopologySnapshot::Fence {
        id: ids
            .allocate(root)
            .map_err(|_| PlacementFailure::InvalidTopology)?,
        definition: fence.definition,
        a,
        b,
        gate: Some(Gate {
            open: false,
            locked: false,
        }),
        wall_top: wall_tops.get(edge).ok().copied(),
    };
    Ok((created, removed, cost))
}

fn prepare_path_construction_snapshots(
    submitted: Option<&PathDrawPlan>,
    preview: &PathPreview,
    index: &TopologyIndex,
    grid: TopologyGrid,
    definitions: WorldDefinitionsView<'_>,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    ids: &mut PersistentIdAllocator,
    root: Entity,
) -> Result<(Vec<TopologySnapshot>, Money), PlacementFailure> {
    let (cells, cost) = if let Some((plan, cost)) = submitted.and_then(|plan| {
        if let openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity::Valid { cost } = plan.validity {
            Some((plan, cost))
        } else {
            None
        }
    }) {
        (plan.approved.clone(), cost)
    } else {
calculate_path_construction_plan(
        preview,
        index,
        grid,
        definitions,
        terrain_index,
        terrain_assets,
        terrain_chunks,
    )?
    };
    let mut snapshots = Vec::with_capacity(cells.len());
    for cell in cells {
        snapshots.push(TopologySnapshot::Path {
            id: ids
                .allocate(root)
                .map_err(|_| PlacementFailure::InvalidTopology)?,
            definition: preview.definition,
            cell,
        });
    }
    Ok((snapshots, cost))
}

fn prepare_objects_stomped_by_ground_path(
    snapshots: &[TopologySnapshot],
    grid: TopologyGrid,
    definitions: WorldDefinitionsView<'_>,
    placement: &PlacedObjectFootprintOccupancyIndex,
    targets: &Query<StompablePlacedObject>,
) -> Result<Vec<PreparedObjectPlacementMutation>, PlacementFailure> {
    let Some(TopologySnapshot::Path { definition, .. }) = snapshots.first() else {
        return Ok(Vec::new());
    };
    let Some(path) = definitions
        .find_path(*definition)
        .filter(|path| !path.elevated)
    else {
        return Ok(Vec::new());
    };
    let Some(policy) = definitions
        .path_stomp_policy(*definition)
        .filter(|policy| !policy.families.is_empty())
    else {
        return Ok(Vec::new());
    };
    let half_width = Vec2::splat(f32::from(path.width_cm) * 0.005);
    let cells = snapshots
        .iter()
        .filter_map(|snapshot| match snapshot {
            TopologySnapshot::Path { cell, .. } if cell.z == 0 => {
                Some(grid.cell_translation(*cell).xz())
            }
            _ => None,
        })
        .flat_map(|centre| {
            let min = (centre - half_width).floor().as_ivec2();
            let max = (centre + half_width).ceil().as_ivec2();
            (min.y..max.y).flat_map(move |y| (min.x..max.x).map(move |x| IVec2::new(x, y)))
        });
    prepare_placement_stomp_removals(
        cells,
        placement,
        targets,
        |_| false,
        |target| {
            let facts = definitions.placement_target_facts(target);
            let types = facts
                .map(|facts| facts.types.as_slice())
                .or_else(|| {
                    definitions
                        .find_object(target)
                        .map(|object| object.behavior_type_identifiers.as_slice())
                })
                .unwrap_or_default();
            policy
                .action_for_target(types, facts.map(|facts| &facts.properties))
                .unwrap_or(PlacementStompAction::Allow)
        },
    )
    .ok_or(PlacementFailure::Occupied)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_topology_construction_transaction(
    mut commands: Commands,
    mut requests: MessageReader<PrepareConstruction>,
    index: Res<TopologyIndex>,
    grid: Res<TopologyGrid>,
    (terrain, terrain_assets, terrain_chunks, ground, paths, terrain_messages): (
        Res<TerrainIndex>,
        Res<Assets<TerrainAsset>>,
        Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
        TankWallGroundSampling,
        Query<&PathTile>,
        Res<Messages<openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged>>,
    ),
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut ids: ResMut<PersistentIdAllocator>,
    previews: Query<
        (
            Option<&FencePreview>,
            Option<&PathPreview>,
            Option<&SubmittedPreview<FenceDrawPlan>>,
            Option<&SubmittedPreview<PathDrawPlan>>,
        ),
        Or<(With<FencePreview>, With<PathPreview>)>,
    >,
    transactions: Query<&WorldMember>,
    persistent: Query<&PersistentId>,
    gate_hosts: GateHostEdges,
    wall_tops: Query<&TankWallTopHeight>,
    (tanks, incident, boundaries): (
        Query<&TankGeometry, With<Tank>>,
        Query<(Option<&FencesStartingHere>, Option<&FencesEndingHere>)>,
        HabitatBoundaryQueries,
    ),
    (mut prepared, mut rejected): (
        MessageWriter<TopologyEditPrepared>,
        MessageWriter<TopologyEditPreparationRejected>,
    ),
    (placement, stomp_targets, mut placement_prepared): (
        Res<PlacedObjectFootprintOccupancyIndex>,
        Query<StompablePlacedObject>,
        MessageWriter<ObjectPlacementEditPrepared>,
    ),
) {
    let version = TopologyPlanVersion {
        revisions: [
            grid.last_changed(),
            index.last_changed(),
            active_definitions.last_changed(),
            definitions.last_changed(),
            terrain.last_changed(),
            terrain_assets.last_changed(),
        ],
        terrain_changes: terrain_change_cursor(&terrain_messages),
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for request in requests.read() {
        let Ok((fence, path, fence_submission, path_submission)) = previews.get(request.preview)
        else {
            continue;
        };
        let fence_plan = fence_submission.map(|submission| &submission.0);
        let path_plan = path_submission.map(|submission| &submission.0);
        let fence = fence_plan.map(|plan| &plan.input).or(fence);
        let path = path_plan.map(|plan| &plan.input).or(path);
        let Ok(member) = transactions.get(request.transaction) else {
            write_topology_edit_preparation_rejection(
                request.transaction,
                PlacementFailure::InvalidTopology,
                &mut rejected,
            );
            continue;
        };
        if terrain.chunks.is_empty() {
            write_topology_edit_preparation_rejection(
                request.transaction,
                PlacementFailure::OutsideMap,
                &mut rejected,
            );
            continue;
        }

        let result = match (fence, path) {
            (Some(preview), None) if fence_definition_is_gate(definitions, preview.definition) => {
                prepare_gate_replacement_snapshots(
                    fence_plan.filter(|plan| plan.version == version && plan.input == *preview),
                    preview,
                    &index,
                    definitions,
                    &gate_hosts,
                    &wall_tops,
                    &persistent,
                    &mut ids,
                    member.root,
                )
                .map(|(created, removed, cost)| (vec![created], vec![removed], cost))
            }
            (Some(preview), None) => prepare_fence_construction_snapshots(
                fence_plan.filter(|plan| plan.version == version && plan.input == *preview),
                preview,
                &index,
                *grid,
                definitions,
                &persistent,
                &terrain,
                &terrain_assets,
                &terrain_chunks,
                &mut ids,
                member.root,
                |cell| {
                    let node = *index.nodes.get(&cell)?;
                    let (starting, ending) = incident.get(node).ok()?;
                    starting
                        .into_iter()
                        .flat_map(|edges| edges.iter())
                        .chain(ending.into_iter().flat_map(|edges| edges.iter()))
                        .flat_map(|edge| boundaries.habitats(edge))
                        .filter_map(|habitat| {
                            tanks
                                .get(habitat)
                                .ok()
                                .map(|geometry| geometry.water_height)
                        })
                        .max_by(f32::total_cmp)
                },
                &ground,
                &paths,
                (&gate_hosts, &wall_tops),
            ),
            (None, Some(preview)) => prepare_path_construction_snapshots(
                path_plan.filter(|plan| plan.version == version && plan.input == *preview),
                preview,
                &index,
                *grid,
                definitions,
                &terrain,
                &terrain_assets,
                &terrain_chunks,
                &mut ids,
                member.root,
            )
            .map(|(created, cost)| (created, Vec::new(), cost)),
            _ => Err(PlacementFailure::InvalidTopology),
        };
        match result {
            Ok((created, removed, cost)) => {
                match prepare_objects_stomped_by_ground_path(
                    &created,
                    *grid,
                    definitions,
                    &placement,
                    &stomp_targets,
                ) {
                    Ok(mutations) if !mutations.is_empty() => {
                        commands
                            .entity(request.transaction)
                            .insert(PreparedObjectPlacementEdit {
                                mutations: mutations.into_boxed_slice(),
                            });
                        placement_prepared.write(ObjectPlacementEditPrepared {
                            transaction: request.transaction,
                            cost: Money::ZERO,
                        });
                    }
                    Ok(_) => {}
                    Err(reason) => {
                        write_topology_edit_preparation_rejection(
                            request.transaction,
                            reason,
                            &mut rejected,
                        );
                        continue;
                    }
                }
                commands.entity(request.transaction).insert(TopologyEdit {
                    created: created.into_boxed_slice(),
                    removed: removed.into_boxed_slice(),
                });
                prepared.write(TopologyEditPrepared {
                    transaction: request.transaction,
                    cost,
                });
            }
            Err(reason) => write_topology_edit_preparation_rejection(
                request.transaction,
                reason,
                &mut rejected,
            ),
        }
    }
}
