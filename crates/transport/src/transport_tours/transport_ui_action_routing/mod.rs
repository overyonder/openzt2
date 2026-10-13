use bevy::prelude::*;
use openzt2_game_data::ui_document::action::transportation::UiTransportAction;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::information::catalogue_types::PurchaseChoice;
use openzt2_interface_state::information::entity_selection_types::SelectedEntity;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;

use openzt2_interface_state::ui::authored_ui_action_projection_components::UiTransportActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitDirection;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitMember;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitVehicleDefinition;
use openzt2_zoo_state::transport_tours::transport_circuit_types::OpenCircuitRequest;
use openzt2_zoo_state::transport_tours::transport_circuit_types::TransportCircuit;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportPathDeletionPending;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::GenerateTransportVehicleForCircuitRequest;

/// Applies the authored vehicle catalogue choice to the selected compatible
/// circuit. The circuit component is the sole owner of that selection.
pub(super) fn select_authored_transport_vehicle_for_selected_compatible_circuit(
    mut purchase_choices: MessageReader<PurchaseChoice>,
    selected_entity: Res<SelectedEntity>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    transport_circuits: Query<&TransportCircuit>,
    transport_circuit_members: Query<&CircuitMember>,
    mut commands: Commands,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(circuit_entity) = selected_entity.0.and_then(|selected_entity| {
        transport_circuits
            .contains(selected_entity)
            .then_some(selected_entity)
            .or_else(|| {
                transport_circuit_members
                    .get(selected_entity)
                    .ok()
                    .map(|membership| membership.0)
            })
    }) else {
        return;
    };
    let Ok(transport_circuit) = transport_circuits.get(circuit_entity) else {
        return;
    };
    let Some(transport_kind) = world_definitions
        .find_station(transport_circuit.definition)
        .map(|station| station.kind)
        .or_else(|| {
            world_definitions
                .find_track(transport_circuit.definition)
                .map(|track| track.kind)
        })
    else {
        return;
    };
    for purchase_choice in purchase_choices.read() {
        if world_definitions
            .find_vehicle(purchase_choice.definition)
            .is_some_and(|vehicle| vehicle.kind == transport_kind)
        {
            commands
                .entity(circuit_entity)
                .insert(CircuitVehicleDefinition(purchase_choice.definition));
        }
    }
}

pub(super) fn route_authored_transport_ui_actions_to_selected_circuit_and_path_deletion_operations(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    action_nodes: Query<(&UiTransportActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    selected_entity: Res<SelectedEntity>,
    circuits: Query<(&TransportCircuit, Option<&CircuitDirection>)>,
    circuit_members: Query<&CircuitMember>,
    vehicle_definitions: Query<&CircuitVehicleDefinition>,
    pending_deletions: Query<&TransportPathDeletionPending>,
    mut open_circuit_requests: MessageWriter<OpenCircuitRequest>,
    mut vehicle_generation_requests: MessageWriter<GenerateTransportVehicleForCircuitRequest>,
    mut deletions: MessageWriter<
        openzt2_zoo_state::construction::construction_interaction_types::DeleteEntity,
    >,
) {
    for activation in activations.read() {
        let Ok((range, owner)) = action_nodes.get(activation.node) else {
            continue;
        };
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let selected_transport_circuit = selected_entity.0.and_then(|selected_entity| {
            circuits
                .contains(selected_entity)
                .then_some(selected_entity)
                .or_else(|| {
                    circuit_members
                        .get(selected_entity)
                        .ok()
                        .map(|membership| membership.0)
                        .filter(|circuit| circuits.contains(*circuit))
                })
        });
        let records = range.authored_action_records(document);
        for record in records {
            if activation.trigger != record.trigger {
                continue;
            }
            match &record.action {
                UiTransportAction::SetSelectedTransportCircuitOpen { open } => {
                    let Some(circuit) = selected_transport_circuit else {
                        continue;
                    };
                    if circuits.contains(circuit) {
                        open_circuit_requests.write(OpenCircuitRequest {
                            circuit,
                            open: *open,
                        });
                    }
                }
                UiTransportAction::ReverseSelectedTransportCircuitDirection => {
                    let Some(circuit) = selected_transport_circuit else {
                        continue;
                    };
                    let direction = circuits
                        .get(circuit)
                        .ok()
                        .and_then(|(_, direction)| direction)
                        .copied()
                        .unwrap_or_default();
                    commands.entity(circuit).insert(match direction {
                        CircuitDirection::Forward => CircuitDirection::Reverse,
                        CircuitDirection::Reverse => CircuitDirection::Forward,
                    });
                }
                UiTransportAction::GenerateVehicleForSelectedTransportCircuit => {
                    let Some(circuit) = selected_transport_circuit else {
                        continue;
                    };
                    if let Ok(definition) = vehicle_definitions.get(circuit) {
                        vehicle_generation_requests.write(
                            GenerateTransportVehicleForCircuitRequest {
                                circuit,
                                definition: definition.0,
                            },
                        );
                    }
                }
                UiTransportAction::ResolvePendingTransportPathDeletion { deletion_confirmed } => {
                    if let Ok(pending) = pending_deletions.get(owner.0) {
                        if *deletion_confirmed && selected_entity.0 == Some(pending.target) {
                            deletions.write(
                                openzt2_zoo_state::construction::construction_interaction_types::DeleteEntity(
                                    pending.target,
                                ),
                            );
                        }
                        commands
                            .entity(owner.0)
                            .remove::<TransportPathDeletionPending>();
                    }
                }
            }
        }
    }
}
