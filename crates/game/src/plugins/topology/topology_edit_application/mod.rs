use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_transaction_types::EditApplication;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    fence_segment_prefab_selection::select_authored_fence_segment_prefab_for_adjacent_topology,
    gate_operation_types::{Gate, GateAutoClose, GateMechanism},
    topology_edit_outcome_messages::write_topology_edit_acknowledgement,
    topology_edit_types::{
        CommitTopologyEdit, TopologyChanged, TopologyEdit, TopologyEditAcknowledged,
        TopologySnapshot,
    },
    topology_graph_types::{
        topology_cell_rectangle, EdgeKey, FenceEdge, PathTile, Portal, TileSurface, TopologyGrid,
        TopologyIndex, TopologyNode,
    },
    topology_grid_geometry::{
        calculate_topology_edge_cell_bounds, calculate_topology_edge_world_transform,
        merge_topology_changed_cell_bounds,
    },
    topology_presentation_types::FenceTerrainEndpointPresentationCells,
};

fn topology_snapshot_application_is_valid(
    add: &[TopologySnapshot],
    remove: &[TopologySnapshot],
    current: &HashMap<u64, Entity>,
    topology: &Query<(
        Option<&TopologyNode>,
        Option<&FenceEdge>,
        Option<&PathTile>,
        Option<&Portal>,
    )>,
    gates: &Query<&Gate>,
    index: &TopologyIndex,
) -> bool {
    for (index, snapshot) in add.iter().enumerate() {
        if current.contains_key(&snapshot.id().0)
            || add[..index].iter().any(|other| other.id() == snapshot.id())
        {
            return false;
        }
    }
    for (index, snapshot) in remove.iter().enumerate() {
        let Some(&entity) = current.get(&snapshot.id().0) else {
            return false;
        };
        if remove[..index]
            .iter()
            .any(|other| other.id() == snapshot.id())
        {
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
                        && current.get(&a.0) == Some(&value.a)
                        && current.get(&b.0) == Some(&value.b)
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
    if !add.iter().all(|snapshot| match *snapshot {
        TopologySnapshot::Fence { a, b, .. } => {
            persistent_node_is_available_after_application(a, add, remove, current, topology)
                && persistent_node_is_available_after_application(b, add, remove, current, topology)
        }
        TopologySnapshot::Portal { destination, .. } => {
            persistent_endpoint_is_available_after_application(
                destination,
                add,
                remove,
                current,
                topology,
            )
        }
        _ => true,
    }) {
        return false;
    }

    add.iter().all(|snapshot| match *snapshot {
        TopologySnapshot::Node { cell, .. } => index.nodes.get(&cell).is_none_or(|entity| {
            snapshot_entity_is_removed_by_application(*entity, remove, current)
        }),
        TopologySnapshot::Path { cell, .. } => index.paths.get(&cell).is_none_or(|entity| {
            snapshot_entity_is_removed_by_application(*entity, remove, current)
        }),
        TopologySnapshot::Fence { a, b, .. } => {
            let Some(a) = resolve_topology_node_cell(a, current, topology, add) else {
                return false;
            };
            let Some(b) = resolve_topology_node_cell(b, current, topology, add) else {
                return false;
            };
            let Some(key) = EdgeKey::new(a, b) else {
                return false;
            };
            index.edges.get(&key).is_none_or(|entity| {
                snapshot_entity_is_removed_by_application(*entity, remove, current)
            })
        }
        TopologySnapshot::Portal { .. } => true,
    })
}

fn snapshot_entity_is_removed_by_application(
    entity: Entity,
    remove: &[TopologySnapshot],
    current: &HashMap<u64, Entity>,
) -> bool {
    remove
        .iter()
        .any(|snapshot| current.get(&snapshot.id().0) == Some(&entity))
}

fn persistent_endpoint_is_available_after_application(
    id: PersistentId,
    add: &[TopologySnapshot],
    remove: &[TopologySnapshot],
    current: &HashMap<u64, Entity>,
    topology: &Query<(
        Option<&TopologyNode>,
        Option<&FenceEdge>,
        Option<&PathTile>,
        Option<&Portal>,
    )>,
) -> bool {
    current.get(&id.0).is_some_and(|entity| {
        !snapshot_entity_is_removed_by_application(*entity, remove, current)
            && topology
                .get(*entity)
                .is_ok_and(|(node, fence, path, portal)| {
                    node.is_some() || fence.is_some() || path.is_some() || portal.is_some()
                })
    }) || add.iter().any(|snapshot| {
        snapshot.id() == id
            && matches!(
                snapshot,
                TopologySnapshot::Node { .. }
                    | TopologySnapshot::Fence { .. }
                    | TopologySnapshot::Path { .. }
            )
    })
}

fn persistent_node_is_available_after_application(
    id: PersistentId,
    add: &[TopologySnapshot],
    remove: &[TopologySnapshot],
    current: &HashMap<u64, Entity>,
    topology: &Query<(
        Option<&TopologyNode>,
        Option<&FenceEdge>,
        Option<&PathTile>,
        Option<&Portal>,
    )>,
) -> bool {
    current
        .get(&id.0)
        .is_some_and(|entity| {
            !snapshot_entity_is_removed_by_application(*entity, remove, current)
                && topology.get(*entity).is_ok_and(|facts| facts.0.is_some())
        })
        || add.iter().any(
            |snapshot| matches!(snapshot, TopologySnapshot::Node { id: candidate, .. } if *candidate == id),
        )
}

fn resolve_topology_node_cell(
    id: PersistentId,
    resolved: &HashMap<u64, Entity>,
    topology: &Query<(
        Option<&TopologyNode>,
        Option<&FenceEdge>,
        Option<&PathTile>,
        Option<&Portal>,
    )>,
    additions: &[TopologySnapshot],
) -> Option<IVec3> {
    additions
        .iter()
        .find_map(|snapshot| match *snapshot {
            TopologySnapshot::Node {
                id: candidate,
                cell,
            } if candidate == id => Some(cell),
            _ => None,
        })
        .or_else(|| {
            let entity = resolved.get(&id.0)?;
            topology.get(*entity).ok()?.0.map(|node| node.cell)
        })
}

fn find_adjacent_added_fence_cell(
    endpoint: PersistentId,
    excluded_endpoint: PersistentId,
    additions: &[TopologySnapshot],
    resolved: &HashMap<u64, Entity>,
    topology: &Query<(
        Option<&TopologyNode>,
        Option<&FenceEdge>,
        Option<&PathTile>,
        Option<&Portal>,
    )>,
) -> Option<IVec3> {
    additions.iter().find_map(|snapshot| {
        let TopologySnapshot::Fence { a, b, .. } = *snapshot else {
            return None;
        };
        let other = if a == endpoint && b != excluded_endpoint {
            b
        } else if b == endpoint && a != excluded_endpoint {
            a
        } else {
            return None;
        };
        resolve_topology_node_cell(other, resolved, topology, additions)
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_committed_topology_edit(
    mut commands: Commands,
    mut requests: MessageReader<CommitTopologyEdit>,
    edits: Query<&TopologyEdit>,
    roots: Query<Entity, With<WorldRoot>>,
    members: Query<&WorldMember>,
    persistent: Query<(Entity, &PersistentId)>,
    topology: Query<(
        Option<&TopologyNode>,
        Option<&FenceEdge>,
        Option<&PathTile>,
        Option<&Portal>,
    )>,
    gates: Query<&Gate>,
    mut index: ResMut<TopologyIndex>,
    grid: Res<TopologyGrid>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    mut changed: MessageWriter<TopologyChanged>,
    mut acknowledged: MessageWriter<TopologyEditAcknowledged>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for request in requests.read() {
        let Ok(edit) = edits.get(request.transaction) else {
            write_topology_edit_acknowledgement(request, false, &mut acknowledged);
            continue;
        };
        let root = members
            .get(request.transaction)
            .ok()
            .map(|member| member.root)
            .or_else(|| roots.iter().next());
        let Some(root) = root else {
            write_topology_edit_acknowledgement(request, false, &mut acknowledged);
            continue;
        };
        let current = persistent
            .iter()
            .map(|(entity, id)| (id.0, entity))
            .collect::<HashMap<_, _>>();
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
            write_topology_edit_acknowledgement(request, false, &mut acknowledged);
            continue;
        };
        if request.application != EditApplication::FailureRollback
            && !topology_snapshot_application_is_valid(
                add, remove, &current, &topology, &gates, &index,
            )
        {
            write_topology_edit_acknowledgement(request, false, &mut acknowledged);
            continue;
        }

        index.nodes.reserve(
            add.iter()
                .filter(|snapshot| matches!(snapshot, TopologySnapshot::Node { .. }))
                .count(),
        );
        index.paths.reserve(
            add.iter()
                .filter(|snapshot| matches!(snapshot, TopologySnapshot::Path { .. }))
                .count(),
        );
        index.edges.reserve(
            add.iter()
                .filter(|snapshot| matches!(snapshot, TopologySnapshot::Fence { .. }))
                .count(),
        );

        let mut resolved = current;
        let mut bounds: Option<IRect> = None;
        for snapshot in remove {
            match *snapshot {
                TopologySnapshot::Node { cell, .. } => {
                    index.nodes.remove(&cell);
                    bounds =
                        merge_topology_changed_cell_bounds(bounds, topology_cell_rectangle(cell));
                }
                TopologySnapshot::Path { cell, .. } => {
                    index.paths.remove(&cell);
                    bounds =
                        merge_topology_changed_cell_bounds(bounds, topology_cell_rectangle(cell));
                }
                TopologySnapshot::Fence { a, b, .. } => {
                    if let (Some(a), Some(b)) = (
                        resolve_topology_node_cell(a, &resolved, &topology, &[]),
                        resolve_topology_node_cell(b, &resolved, &topology, &[]),
                    ) {
                        index
                            .edges
                            .remove(&EdgeKey::new(a, b).expect("validated endpoints"));
                        bounds = merge_topology_changed_cell_bounds(
                            bounds,
                            calculate_topology_edge_cell_bounds(a, b),
                        );
                    }
                }
                TopologySnapshot::Portal { .. } => {}
            }
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
            index.nodes.insert(cell, entity);
            bounds = merge_topology_changed_cell_bounds(bounds, topology_cell_rectangle(cell));
        }
        for snapshot in add {
            match *snapshot {
                TopologySnapshot::Node { .. } => {}
                TopologySnapshot::Fence {
                    id,
                    definition,
                    a,
                    b,
                    gate,
                } => {
                    let (Some(&a_entity), Some(&b_entity)) =
                        (resolved.get(&a.0), resolved.get(&b.0))
                    else {
                        unreachable!()
                    };
                    let a_cell = resolve_topology_node_cell(a, &resolved, &topology, add)
                        .expect("validated endpoint");
                    let b_cell = resolve_topology_node_cell(b, &resolved, &topology, add)
                        .expect("validated endpoint");
                    let entity = commands
                        .spawn((
                            FenceEdge {
                                definition,
                                a: a_entity,
                                b: b_entity,
                            },
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
                    if let Some(fence) = definitions.find_fence(definition) {
                        let next = find_adjacent_added_fence_cell(b, a, add, &resolved, &topology);
                        let prefab = select_authored_fence_segment_prefab_for_adjacent_topology(
                            fence, a_cell, b_cell, next,
                        );
                        if prefab.asset != AssetId::default() {
                            commands.entity(entity).insert(prefab);
                        }
                        if gate.is_some() {
                            let policy = &fence.gate_policy;
                            commands.entity(entity).insert((
                                GateMechanism {
                                    prefab: AssetId(policy.prefab.0),
                                    open_animation: AssetId(policy.open_animation.0),
                                    close_animation: AssetId(policy.close_animation.0),
                                    trigger_distance_cm: policy.trigger_distance_cm,
                                    auto_close_ticks: policy.auto_close_ticks,
                                },
                                GateAutoClose(0),
                            ));
                        }
                    }
                    index.edges.insert(
                        EdgeKey::new(a_cell, b_cell).expect("distinct endpoints"),
                        entity,
                    );
                    resolved.insert(id.0, entity);
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
                    index.paths.insert(cell, entity);
                    resolved.insert(id.0, entity);
                    bounds =
                        merge_topology_changed_cell_bounds(bounds, topology_cell_rectangle(cell));
                }
                TopologySnapshot::Portal { .. } => {}
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
            let destination = resolved[&destination.0];
            let entity = commands
                .spawn((
                    Portal {
                        destination,
                        bidirectional,
                    },
                    id,
                    WorldMember { root },
                ))
                .id();
            resolved.insert(id.0, entity);
        }

        for snapshot in remove {
            let entity = resolved[&snapshot.id().0];
            match *snapshot {
                TopologySnapshot::Node { cell, .. } => {
                    bounds =
                        merge_topology_changed_cell_bounds(bounds, topology_cell_rectangle(cell));
                }
                TopologySnapshot::Path { cell, .. } => {
                    bounds =
                        merge_topology_changed_cell_bounds(bounds, topology_cell_rectangle(cell));
                }
                TopologySnapshot::Fence { a, b, .. } => {
                    if let (Some(a), Some(b)) = (
                        resolve_topology_node_cell(a, &resolved, &topology, add),
                        resolve_topology_node_cell(b, &resolved, &topology, add),
                    ) {
                        bounds = merge_topology_changed_cell_bounds(
                            bounds,
                            calculate_topology_edge_cell_bounds(a, b),
                        );
                    }
                }
                TopologySnapshot::Portal { .. } => {}
            }
            commands.entity(entity).despawn();
        }

        if let Some(bounds) = bounds {
            changed.write(TopologyChanged {
                transaction: Some(request.transaction),
                bounds,
            });
        }
        write_topology_edit_acknowledgement(request, true, &mut acknowledged);
    }
}
