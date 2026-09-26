use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::construction::construction_interaction_types::PlacementFailure;
use crate::plugins::construction::construction_transaction_types::PrepareDeletion;
use crate::plugins::economy::money_types::Money;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;

use super::{
    gate_operation_types::Gate,
    topology_edit_outcome_messages::write_topology_edit_preparation_rejection,
    topology_edit_types::{
        TopologyEdit, TopologyEditPreparationRejected, TopologyEditPrepared, TopologySnapshot,
    },
    topology_graph_types::{FenceEdge, PathTile, Portal, TopologyNode, TopologyProtected},
};

fn calculate_topology_snapshot_deletion_refund(
    snapshots: &[TopologySnapshot],
    definitions: WorldDefinitionsView<'_>,
) -> Option<i64> {
    snapshots
        .iter()
        .filter_map(|snapshot| match *snapshot {
            TopologySnapshot::Fence { definition, .. } => Some((definition, true)),
            TopologySnapshot::Path { definition, .. } => Some((definition, false)),
            TopologySnapshot::Node { .. } | TopologySnapshot::Portal { .. } => None,
        })
        .try_fold(0_i64, |total, (definition, fence)| {
            let object = if fence {
                &definitions.find_fence(definition)?.object
            } else {
                &definitions.find_path(definition)?.object
            };
            let object = definitions.find_object(AssetId(object.0))?;
            total.checked_add(object.price_cents)
        })?
        .checked_neg()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_topology_deletion_transaction(
    mut commands: Commands,
    mut requests: MessageReader<PrepareDeletion>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    topology: Query<(
        Entity,
        Option<&TopologyNode>,
        Option<&FenceEdge>,
        Option<&PathTile>,
        Option<&Gate>,
        Option<&Portal>,
        Option<&TopologyProtected>,
        &PersistentId,
    )>,
    persistent: Query<(Entity, &PersistentId)>,
    mut prepared: MessageWriter<TopologyEditPrepared>,
    mut rejected: MessageWriter<TopologyEditPreparationRejected>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for request in requests.read() {
        let Ok((_, node, fence, path, gate, portal, protected, id)) = topology.get(request.target)
        else {
            continue;
        };
        if protected.is_some() {
            write_topology_edit_preparation_rejection(
                request.transaction,
                PlacementFailure::InvalidTopology,
                &mut rejected,
            );
            continue;
        }
        let mut removed = Vec::new();
        if let Some(node) = node {
            removed.push(TopologySnapshot::Node {
                id: *id,
                cell: node.cell,
            });
        }
        if let Some(fence) = fence {
            let Some(a) = persistent.get(fence.a).ok().map(|(_, id)| *id) else {
                write_topology_edit_preparation_rejection(
                    request.transaction,
                    PlacementFailure::InvalidTopology,
                    &mut rejected,
                );
                continue;
            };
            let Some(b) = persistent.get(fence.b).ok().map(|(_, id)| *id) else {
                write_topology_edit_preparation_rejection(
                    request.transaction,
                    PlacementFailure::InvalidTopology,
                    &mut rejected,
                );
                continue;
            };
            removed.push(TopologySnapshot::Fence {
                id: *id,
                definition: fence.definition,
                a,
                b,
                gate: gate.copied(),
            });
        }
        if let Some(path) = path {
            removed.push(TopologySnapshot::Path {
                id: *id,
                definition: path.definition,
                cell: path.cell,
            });
        }
        if let Some(portal) = portal {
            let Some(destination) = persistent.get(portal.destination).ok().map(|(_, id)| *id)
            else {
                write_topology_edit_preparation_rejection(
                    request.transaction,
                    PlacementFailure::InvalidTopology,
                    &mut rejected,
                );
                continue;
            };
            removed.push(TopologySnapshot::Portal {
                id: *id,
                destination,
                bidirectional: portal.bidirectional,
            });
        }

        let mut valid = true;
        for (
            dependent_entity,
            _,
            edge,
            _,
            edge_gate,
            dependent_portal,
            dependent_protected,
            dependent_id,
        ) in &topology
        {
            if node.is_some() {
                if let Some(edge) = edge {
                    if edge.a != request.target && edge.b != request.target {
                        // This edge is unrelated, but its portal may still
                        // target the entity being deleted.
                    } else {
                        if dependent_protected.is_some() {
                            valid = false;
                            break;
                        }
                        let Some(a) = persistent.get(edge.a).ok().map(|(_, id)| *id) else {
                            valid = false;
                            break;
                        };
                        let Some(b) = persistent.get(edge.b).ok().map(|(_, id)| *id) else {
                            valid = false;
                            break;
                        };
                        if !removed
                            .iter()
                            .any(|snapshot| snapshot.id() == *dependent_id)
                        {
                            removed.push(TopologySnapshot::Fence {
                                id: *dependent_id,
                                definition: edge.definition,
                                a,
                                b,
                                gate: edge_gate.copied(),
                            });
                        }
                    }
                }
            }
            if dependent_entity != request.target
                && dependent_portal.is_some_and(|portal| {
                    portal.destination == request.target
                        && !removed
                            .iter()
                            .any(|snapshot| snapshot.id() == *dependent_id)
                })
            {
                let portal = dependent_portal.expect("checked above");
                removed.push(TopologySnapshot::Portal {
                    id: *dependent_id,
                    destination: *id,
                    bidirectional: portal.bidirectional,
                });
            }
        }
        if !valid {
            write_topology_edit_preparation_rejection(
                request.transaction,
                PlacementFailure::InvalidTopology,
                &mut rejected,
            );
            continue;
        }

        if removed.is_empty() {
            write_topology_edit_preparation_rejection(
                request.transaction,
                PlacementFailure::InvalidTopology,
                &mut rejected,
            );
            continue;
        }
        let Some(refund) = calculate_topology_snapshot_deletion_refund(&removed, definitions)
        else {
            write_topology_edit_preparation_rejection(
                request.transaction,
                PlacementFailure::InvalidTopology,
                &mut rejected,
            );
            continue;
        };
        commands.entity(request.transaction).insert(TopologyEdit {
            created: Box::new([]),
            removed: removed.into_boxed_slice(),
        });
        prepared.write(TopologyEditPrepared {
            transaction: request.transaction,
            cost: Money(refund),
        });
    }
}
