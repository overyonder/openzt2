use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

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
use openzt2_zoo_state::topology::gate_operation_types::GateAutoClose;
use openzt2_zoo_state::topology::gate_operation_types::GateMechanism;
use openzt2_zoo_state::topology::topology_edit_types::CommitTopologyEdit;
use openzt2_zoo_state::topology::topology_edit_types::TopologyChanged;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEdit;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditAcknowledged;
use openzt2_zoo_state::topology::topology_edit_types::TopologySnapshot;
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
    edits: Query<(&TopologyEdit, Option<&TankTopologyRestorations>)>,
    tanks: Query<(
        &Tank,
        &TankGeometry,
        &WaterQuality,
        &HabitatRegion,
        &WorldMember,
    )>,
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
        let Ok((edit, restorations)) = edits.get(request.transaction) else {
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

        let mut restorations = restorations.cloned().unwrap_or_default();
        let mut captured = Vec::new();
        for (tank, geometry, quality, region, member) in &tanks {
            if member.root != root
                || !remove.iter().any(|snapshot| {
                    current
                        .get(&snapshot.id().0)
                        .is_some_and(|entity| region.boundary_fence_entities.contains(entity))
                })
            {
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
        commands.entity(request.transaction).insert(restorations);
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
                    wall_top,
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
                    if let Some(top) = wall_top {
                        commands.entity(entity).insert(top);
                    }
                    let fence = if gate.is_some() {
                        definitions.find_gate_fence(definition)
                    } else {
                        definitions.find_fence(definition)
                    };
                    if let Some(fence) = fence {
                        let next = find_adjacent_added_fence_cell(b, a, add, &resolved, &topology);
                        let prefab = select_authored_fence_segment_prefab_for_adjacent_topology(
                            fence, a_cell, b_cell, next,
                        );
                        if prefab.asset != AssetId::default() {
                            commands.entity(entity).insert(prefab);
                        }
                        if gate.is_some() {
                            commands
                                .entity(entity)
                                .insert((GateMechanism::default(), GateAutoClose(0)));
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
