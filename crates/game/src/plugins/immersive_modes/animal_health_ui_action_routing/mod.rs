//! Authored animal-health tool actions applied to the activated UI controller.

use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    action::animal_health::UiAnimalHealthAction, document::UiDocumentRole,
};

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::{
        information::entity_selection_types::SelectedEntity,
        ui::{
            authored_ui_action_projection_components::UiAnimalHealthActions,
            authored_ui_activation_contracts::UiNodeActivated,
            authored_ui_node_projection_components::UiDocumentOwner,
            authored_ui_node_projection_components::UiDocumentRoot,
            ui_document_lifecycle_contracts::ShowUiRole,
        },
    },
};

use super::animal_care_control_types::{
    AnimalCareTarget, DiseaseTreatmentControl, TranquilizerControl,
};

pub(super) fn route_animal_health_tool_actions_from_activated_ui_nodes(
    mut commands: Commands,
    mut ui_node_activations: MessageReader<UiNodeActivated>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    ui_nodes: Query<(&UiAnimalHealthActions, &UiDocumentOwner)>,
    ui_document_roots: Query<&UiDocumentRoot>,
    selected_entity: Res<SelectedEntity>,
    mut show_ui_document_requests: MessageWriter<ShowUiRole>,
) {
    for activation in ui_node_activations.read() {
        let Ok((action_records, document_owner)) = ui_nodes.get(activation.node) else {
            continue;
        };
        let Ok(document_root) = ui_document_roots.get(document_owner.0) else {
            continue;
        };
        let Some(document) = ui_document_assets.get(&document_root.document) else {
            continue;
        };
        for action_record in action_records.authored_action_records(document) {
            if activation.trigger != action_record.trigger {
                continue;
            }
            match &action_record.action {
                UiAnimalHealthAction::EnterDiseaseTreatmentMode => {
                    commands.entity(activation.node).insert((
                        DiseaseTreatmentControl,
                        AnimalCareTarget {
                            animal_entity: selected_entity.0,
                        },
                    ));
                }
                UiAnimalHealthAction::EnterTranquilizerMode => {
                    commands.entity(activation.node).insert((
                        TranquilizerControl,
                        AnimalCareTarget {
                            animal_entity: selected_entity.0,
                        },
                    ));
                    show_ui_document_requests.write(ShowUiRole {
                        role: UiDocumentRole::TranquilizerHud,
                        owner: activation.node,
                    });
                }
            }
        }
    }
}
