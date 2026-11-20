use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::TankWallTopHeight;
use openzt2_zoo_state::topology::topology_graph_types::{
    FenceSegment, FencesEndingHere, FencesStartingHere, IncomingPortals,
};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareDeletion;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;

use crate::topology::topology_edit_outcome_messages::write_topology_edit_preparation_rejection;
use openzt2_zoo_state::topology::gate_operation_types::Gate;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEdit;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPreparationRejected;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPrepared;
use openzt2_zoo_state::topology::topology_edit_types::TopologySnapshot;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::topology::topology_graph_types::Portal;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;
use openzt2_zoo_state::topology::topology_graph_types::TopologyProtected;

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
            // The authored `destroy` credit, 80% of the price for walls.
            let refund =
                openzt2_zoo_state::economy::object_sale_refund::calculate_authored_object_sale_refund(
                    object,
                )?;
            total.checked_add(refund.0)
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
        Option<FenceSegment>,
        Option<&PathTile>,
        Option<&Gate>,
        Option<&Portal>,
        Option<&TopologyProtected>,
        &PersistentId,
    )>,
    persistent: Query<(Entity, &PersistentId)>,
    dependents: Query<(
        Option<&FencesStartingHere>,
        Option<&FencesEndingHere>,
        Option<&IncomingPortals>,
    )>,
    wall_tops: Query<&TankWallTopHeight>,
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
        if node.is_none() && fence.is_none() && path.is_none() && portal.is_none() {
            continue;
        }
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
            let Some(a) = persistent.get(fence.a()).ok().map(|(_, id)| *id) else {
                write_topology_edit_preparation_rejection(
                    request.transaction,
                    PlacementFailure::InvalidTopology,
                    &mut rejected,
                );
                continue;
            };
            let Some(b) = persistent.get(fence.b()).ok().map(|(_, id)| *id) else {
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
                wall_top: wall_tops.get(request.target).ok().copied(),
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
        let mut visited = bevy::ecs::entity::EntityHashSet::new();
        visited.insert(request.target);
        let mut dependencies = vec![request.target];
        while let Some(target) = dependencies.pop() {
            let Ok((starting, ending, portals)) = dependents.get(target) else {
                continue;
            };
            for entity in starting
                .into_iter()
                .flat_map(|links| links.iter())
                .chain(ending.into_iter().flat_map(|links| links.iter()))
                .chain(portals.into_iter().flat_map(|links| links.iter()))
            {
                if !visited.insert(entity) {
                    continue;
                }
                let Ok((_, _, edge, _, gate, portal, protected, id)) = topology.get(entity) else {
                    continue;
                };
                if protected.is_some() {
                    valid = false;
                    break;
                }
                if let Some(edge) = edge {
                    let (Ok((_, a)), Ok((_, b))) =
                        (persistent.get(edge.a()), persistent.get(edge.b()))
                    else {
                        valid = false;
                        break;
                    };
                    removed.push(TopologySnapshot::Fence {
                        id: *id,
                        definition: edge.definition,
                        a: *a,
                        b: *b,
                        gate: gate.copied(),
                        wall_top: wall_tops.get(entity).ok().copied(),
                    });
                }
                if let Some(portal) = portal {
                    let Ok((_, destination)) = persistent.get(portal.destination) else {
                        valid = false;
                        break;
                    };
                    removed.push(TopologySnapshot::Portal {
                        id: *id,
                        destination: *destination,
                        bidirectional: portal.bidirectional,
                    });
                }
                dependencies.push(entity);
            }
            if !valid {
                break;
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
