use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::topology::habitat_boundary_queries::HabitatBoundaryQueries;
use openzt2_zoo_state::topology::persistent_edit_lookup::PersistentEditLookup;
use openzt2_zoo_state::topology::topology_graph_types::FenceSegment;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use crate::topology::topology_edit_outcome_messages::write_topology_edit_acknowledgement;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::{
    tank_habitat_seed_cell, PendingTankRestoration, Tank, TankGeometry, TankTopologyRestorations,
    WaterQuality,
};
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::topology::fence_segment_prefab_selection::select_authored_fence_segment_prefab_for_adjacent_topology;
use openzt2_zoo_state::topology::fence_segment_presentation_types::FenceTerrainEndpointPresentationCells;
use openzt2_zoo_state::topology::gate_operation_types::Gate;
use openzt2_zoo_state::topology::gate_operation_types::GateMechanism;
use openzt2_zoo_state::topology::topology_edit_types::TopologyChanged;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEdit;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditAcknowledged;
use openzt2_zoo_state::topology::topology_edit_types::TopologySnapshot;
use openzt2_zoo_state::topology::topology_edit_types::{AppliedFenceTopology, CommitTopologyEdit};
use openzt2_zoo_state::topology::topology_graph_types::topology_cell_rectangle;
use openzt2_zoo_state::topology::topology_graph_types::EdgeKey;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::topology::topology_graph_types::Portal;
use openzt2_zoo_state::topology::topology_graph_types::TileSurface;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;
use openzt2_zoo_state::topology::topology_grid_geometry::calculate_topology_edge_cell_bounds;
use openzt2_zoo_state::topology::topology_grid_geometry::calculate_topology_edge_world_transform;
use openzt2_zoo_state::topology::topology_grid_geometry::merge_topology_changed_cell_bounds;

fn topology_snapshot_application_is_valid(
    add: &[TopologySnapshot],
    remove: &[TopologySnapshot],
    current: &HashMap<u64, Entity>,
    topology: &Query<(
        Option<&TopologyNode>,
        Option<FenceSegment>,
        Option<&PathTile>,
        Option<&Portal>,
    )>,
    gates: &Query<&Gate>,
    index: &TopologyIndex,
) -> bool {
    let mut added_ids = HashSet::new();
    for snapshot in add {
        if current.contains_key(&snapshot.id().0) || !added_ids.insert(snapshot.id().0) {
            return false;
        }
    }
    let mut removed_entities = bevy::ecs::entity::EntityHashSet::default();
    for snapshot in remove {
        let Some(&entity) = current.get(&snapshot.id().0) else {
            return false;
        };
        if !removed_entities.insert(entity) {
            return false;
        }
        let Ok((node, fence, path, portal)) = topology.get(entity) else {
            return false;
        };
        let matches = match snapshot {
            TopologySnapshot::Node { cell, .. } => node.is_some_and(|value| value.cell == *cell),
            TopologySnapshot::Fence {
                definition,
                a,
                b,
                gate,
                ..
            } => {
                fence.is_some_and(|value| {
                    value.definition == *definition
                        && current.get(&a.0) == Some(&value.a())
                        && current.get(&b.0) == Some(&value.b())
                }) && gates.get(entity).ok().copied() == *gate
            }
            TopologySnapshot::Path {
                definition, cell, ..
            } => path.is_some_and(|value| value.definition == *definition && value.cell == *cell),
            TopologySnapshot::Portal {
                destination,
                bidirectional,
                ..
            } => portal.is_some_and(|value| {
                value.bidirectional == *bidirectional
                    && current.get(&destination.0) == Some(&value.destination)
            }),
        };
        if !matches {
            return false;
        }
    }
    let node_additions = added_node_cells(add);
    let surviving = |id: PersistentId| {
        current
            .get(&id.0)
            .copied()
            .filter(|entity| !removed_entities.contains(entity))
    };
    let node_available = |id: PersistentId| {
        node_additions.contains_key(&id.0)
            || surviving(id)
                .is_some_and(|entity| topology.get(entity).is_ok_and(|facts| facts.0.is_some()))
    };
    if !add.iter().all(|snapshot| match *snapshot {
        TopologySnapshot::Fence { a, b, .. } => node_available(a) && node_available(b),
        TopologySnapshot::Portal {
            id, destination, ..
        } => {
            id != destination
                && (added_ids.contains(&destination.0)
                    || surviving(destination).is_some_and(|entity| {
                        topology
                            .get(entity)
                            .is_ok_and(|(node, fence, path, portal)| {
                                node.is_some()
                                    || fence.is_some()
                                    || path.is_some()
                                    || portal.is_some()
                            })
                    }))
        }
        _ => true,
    }) {
        return false;
    }

    let mut added_nodes = HashSet::new();
    let mut added_paths = HashSet::new();
    let mut added_edges = HashSet::new();
    add.iter().all(|snapshot| match *snapshot {
        TopologySnapshot::Node { cell, .. } => {
            added_nodes.insert(cell)
                && index
                    .nodes
                    .get(&cell)
                    .is_none_or(|entity| removed_entities.contains(entity))
        }
        TopologySnapshot::Path { cell, .. } => {
            added_paths.insert(cell)
                && index
                    .paths
                    .get(&cell)
                    .is_none_or(|entity| removed_entities.contains(entity))
        }
        TopologySnapshot::Fence { a, b, .. } => {
            let Some(a) = resolve_topology_node_cell(a, current, topology, &node_additions) else {
                return false;
            };
            let Some(b) = resolve_topology_node_cell(b, current, topology, &node_additions) else {
                return false;
            };
            let Some(key) = EdgeKey::new(a, b) else {
                return false;
            };
            added_edges.insert(key)
                && index
                    .edges
                    .get(&key)
                    .is_none_or(|entity| removed_entities.contains(entity))
        }
        TopologySnapshot::Portal { .. } => true,
    })
}

fn added_node_cells(snapshots: &[TopologySnapshot]) -> HashMap<u64, IVec3> {
    snapshots
        .iter()
        .filter_map(|snapshot| match *snapshot {
            TopologySnapshot::Node { id, cell } => Some((id.0, cell)),
            _ => None,
        })
        .collect()
}

fn resolve_topology_node_cell(
    id: PersistentId,
    resolved: &HashMap<u64, Entity>,
    topology: &Query<(
        Option<&TopologyNode>,
        Option<FenceSegment>,
        Option<&PathTile>,
        Option<&Portal>,
    )>,
    additions: &HashMap<u64, IVec3>,
) -> Option<IVec3> {
    additions.get(&id.0).copied().or_else(|| {
        let entity = resolved.get(&id.0)?;
        topology.get(*entity).ok()?.0.map(|node| node.cell)
    })
}

pub(super) fn apply_committed_topology_edit(
    mut requests: MessageReader<CommitTopologyEdit>,
    mut commands: Commands,
) {
    for &request in requests.read() {
        commands.run_system_cached_with(apply_one_committed_topology_edit, request);
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_one_committed_topology_edit(
    In(request): In<CommitTopologyEdit>,
    mut commands: Commands,
    mut edits: Query<(
        &TopologyEdit,
        Option<&mut TankTopologyRestorations>,
        Option<&mut AppliedFenceTopology>,
    )>,
    tanks: Query<(
        &Tank,
        &TankGeometry,
        &WaterQuality,
        &HabitatRegion,
        &WorldMember,
    )>,
    (roots, boundaries): (Query<Entity, With<WorldRoot>>, HabitatBoundaryQueries),
    members: Query<&WorldMember>,
    persistent: Res<PersistentEditLookup>,
    topology: Query<(
        Option<&TopologyNode>,
        Option<FenceSegment>,
        Option<&PathTile>,
        Option<&Portal>,
    )>,
    gates: Query<&Gate>,
    index: Res<TopologyIndex>,
    grid: Res<TopologyGrid>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    mut acknowledged: MessageWriter<TopologyEditAcknowledged>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Ok((edit, live_restorations, applied_fences)) = edits.get_mut(request.transaction) else {
        write_topology_edit_acknowledgement(&request, false, &mut acknowledged);
        return;
    };
    let root = members
        .get(request.transaction)
        .ok()
        .map(|member| member.root)
        .or_else(|| roots.iter().next());
    let Some(root) = root else {
        write_topology_edit_acknowledgement(&request, false, &mut acknowledged);
        return;
    };
    let mut current = HashMap::new();
    for snapshot in edit.created.iter().chain(&edit.removed) {
        let operands = match *snapshot {
            TopologySnapshot::Fence { a, b, .. } => [Some(a), Some(b)],
            TopologySnapshot::Portal { destination, .. } => [Some(destination), None],
            _ => [None, None],
        };
        for id in std::iter::once(snapshot.id()).chain(operands.into_iter().flatten()) {
            if let Some(entity) = persistent.get(root, id) {
                current.insert(id.0, entity);
            }
        }
    }
    let forward_pair = (&edit.created, &edit.removed);
    let inverse_pair = (&edit.removed, &edit.created);
    let direction = match request.application {
        EditApplication::InitialCommit | EditApplication::Redo => Some(forward_pair),
        EditApplication::Undo => Some(inverse_pair),
        EditApplication::FailureRollback => {
            let forward_valid = topology_snapshot_application_is_valid(
                forward_pair.0,
                forward_pair.1,
                &current,
                &topology,
                &gates,
                &index,
            );
            let inverse_valid = topology_snapshot_application_is_valid(
                inverse_pair.0,
                inverse_pair.1,
                &current,
                &topology,
                &gates,
                &index,
            );
            match (forward_valid, inverse_valid) {
                (true, false) => Some(forward_pair),
                (false, true) => Some(inverse_pair),
                _ => None,
            }
        }
    };
    let Some((add, remove)) = direction else {
        write_topology_edit_acknowledgement(&request, false, &mut acknowledged);
        return;
    };
    if request.application != EditApplication::FailureRollback
        && !topology_snapshot_application_is_valid(add, remove, &current, &topology, &gates, &index)
    {
        write_topology_edit_acknowledgement(&request, false, &mut acknowledged);
        return;
    }

    let mut restorations = live_restorations.as_deref().cloned().unwrap_or_default();
    let mut captured = Vec::new();
    let affected_tanks: bevy::ecs::entity::EntityHashSet = remove
        .iter()
        .filter_map(|snapshot| current.get(&snapshot.id().0).copied())
        .flat_map(|entity| boundaries.habitats(entity))
        .collect();
    for (tank, geometry, quality, region, member) in
        tanks.iter_many(affected_tanks).filter_map(Result::ok)
    {
        if member.root != root {
            continue;
        }
        let Some(seed_cell) = tank_habitat_seed_cell(region) else {
            continue;
        };
        captured.push(PendingTankRestoration {
            seed_cell,
            definition: tank.definition,
            geometry: *geometry,
            water_quality: *quality,
            water_limit_missing: false,
        });
    }
    let restoring_created = std::ptr::eq(add, &edit.created);
    let restore = if restoring_created {
        &restorations.created
    } else {
        &restorations.removed
    };
    for image in restore.iter() {
        commands.spawn((*image, WorldMember { root }));
    }
    if !captured.is_empty() {
        if restoring_created {
            restorations.removed = captured.into_boxed_slice();
        } else {
            restorations.created = captured.into_boxed_slice();
        }
    }
    if let Some(mut live_restorations) = live_restorations {
        *live_restorations = restorations;
    } else {
        commands.entity(request.transaction).insert(restorations);
    }
    let mut resolved = current;
    let node_additions = added_node_cells(add);
    let mut joined_endpoints: HashMap<u64, Vec<PersistentId>> = HashMap::new();
    for snapshot in add {
        if let TopologySnapshot::Fence { a, b, .. } = *snapshot {
            joined_endpoints.entry(a.0).or_default().push(b);
            joined_endpoints.entry(b.0).or_default().push(a);
        }
    }
    let no_additions = HashMap::new();
    let mut bounds: Option<IRect> = None;
    for snapshot in remove {
        let entity = resolved[&snapshot.id().0];
        match *snapshot {
            TopologySnapshot::Node { cell, .. } => {
                bounds = merge_topology_changed_cell_bounds(bounds, topology_cell_rectangle(cell));
            }
            TopologySnapshot::Path { cell, .. } => {
                bounds = merge_topology_changed_cell_bounds(bounds, topology_cell_rectangle(cell));
            }
            TopologySnapshot::Fence { a, b, .. } => {
                if let (Some(a), Some(b)) = (
                    resolve_topology_node_cell(a, &resolved, &topology, &no_additions),
                    resolve_topology_node_cell(b, &resolved, &topology, &no_additions),
                ) {
                    bounds = merge_topology_changed_cell_bounds(
                        bounds,
                        calculate_topology_edge_cell_bounds(a, b),
                    );
                }
            }
            TopologySnapshot::Portal { .. } => {}
        }
        commands.entity(entity).try_despawn();
    }

    for snapshot in add
        .iter()
        .filter(|snapshot| matches!(snapshot, TopologySnapshot::Node { .. }))
    {
        let TopologySnapshot::Node { id, cell } = *snapshot else {
            unreachable!()
        };
        let entity = commands
            .spawn((TopologyNode { cell }, id, WorldMember { root }))
            .id();
        resolved.insert(id.0, entity);
        bounds = merge_topology_changed_cell_bounds(bounds, topology_cell_rectangle(cell));
    }
    let mut created_fences = Vec::new();
    for snapshot in add {
        match *snapshot {
            TopologySnapshot::Node { .. } => {}
            TopologySnapshot::Fence {
                id,
                definition,
                a,
                b,
                gate,
                wall_top,
            } => {
                let (Some(&a_entity), Some(&b_entity)) = (resolved.get(&a.0), resolved.get(&b.0))
                else {
                    unreachable!()
                };
                let a_cell = resolve_topology_node_cell(a, &resolved, &topology, &node_additions)
                    .expect("validated endpoint");
                let b_cell = resolve_topology_node_cell(b, &resolved, &topology, &node_additions)
                    .expect("validated endpoint");
                let entity = commands
                    .spawn((
                        FenceEdge::between(definition, a_entity, b_entity),
                        id,
                        WorldMember { root },
                        FenceTerrainEndpointPresentationCells {
                            first: a_cell,
                            second: b_cell,
                        },
                        calculate_topology_edge_world_transform(a_cell, b_cell, *grid),
                    ))
                    .id();
                if let Some(gate) = gate {
                    commands.entity(entity).insert(gate);
                }
                if let Some(top) = wall_top {
                    commands.entity(entity).insert(top);
                }
                let fence = if gate.is_some() {
                    definitions.find_gate_fence(definition)
                } else {
                    definitions.find_fence(definition)
                };
                if let Some(fence) = fence {
                    let next = joined_endpoints
                        .get(&b.0)
                        .into_iter()
                        .flatten()
                        .copied()
                        .filter(|&other| other != a)
                        .find_map(|other| {
                            resolve_topology_node_cell(other, &resolved, &topology, &node_additions)
                        });
                    let prefab = select_authored_fence_segment_prefab_for_adjacent_topology(
                        fence, a_cell, b_cell, next,
                    );
                    if prefab.asset != AssetId::default() {
                        commands.entity(entity).insert(prefab);
                    }
                    if gate.is_some() {
                        commands.entity(entity).insert(GateMechanism::default());
                    }
                }
                resolved.insert(id.0, entity);
                created_fences.push(entity);
                bounds = merge_topology_changed_cell_bounds(
                    bounds,
                    calculate_topology_edge_cell_bounds(a_cell, b_cell),
                );
            }
            TopologySnapshot::Path {
                id,
                definition,
                cell,
            } => {
                let entity = commands
                    .spawn((
                        PathTile { definition, cell },
                        id,
                        WorldMember { root },
                        Transform::from_translation(grid.cell_translation(cell)),
                    ))
                    .id();
                if let Some(path) = definitions.find_path(definition) {
                    commands.entity(entity).insert(TileSurface {
                        height_cm: path.surface.height_cm,
                    });
                }
                resolved.insert(id.0, entity);
                bounds = merge_topology_changed_cell_bounds(bounds, topology_cell_rectangle(cell));
            }
            TopologySnapshot::Portal { .. } => {}
        }
    }
    for snapshot in add {
        if let TopologySnapshot::Portal { id, .. } = *snapshot {
            let entity = commands.spawn((id, WorldMember { root })).id();
            resolved.insert(id.0, entity);
        }
    }
    for snapshot in add {
        let TopologySnapshot::Portal {
            id,
            destination,
            bidirectional,
        } = *snapshot
        else {
            continue;
        };
        commands.entity(resolved[&id.0]).insert(Portal {
            destination: resolved[&destination.0],
            bidirectional,
        });
    }

    if request.application == EditApplication::InitialCommit && !created_fences.is_empty() {
        let fences = AppliedFenceTopology(created_fences.into_boxed_slice());
        if let Some(mut current) = applied_fences {
            *current = fences;
        } else {
            commands.entity(request.transaction).insert(fences);
        }
    }
    if let Some(bounds) = bounds {
        commands.write_message(TopologyChanged {
            transaction: Some(request.transaction),
            bounds,
        });
    }
    commands.write_message(TopologyEditAcknowledged {
        transaction: request.transaction,
        application: request.application,
        accepted: true,
    });
}
