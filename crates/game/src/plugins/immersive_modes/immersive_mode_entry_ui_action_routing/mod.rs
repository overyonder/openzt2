use bevy::prelude::*;
use openzt2_game_data::ui_document::action::immersive_mode::UiImmersiveModeKind;

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::{
        information::entity_selection_types::SelectedEntity,
        ui::{
            authored_ui_node_projection_components::UiDocumentOwner,
            authored_ui_node_projection_components::UiDocumentRoot,
        },
    },
};

use super::{
    immersive_mode_message_types::EnterImmersiveMode, immersive_mode_state_types::ImmersiveMode,
};
use crate::plugins::ui::authored_ui_action_projection_components::UiImmersiveModeActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(super) fn route_authored_immersive_mode_entry_actions_from_activated_ui_nodes(
    mut ui_node_activations: MessageReader<UiNodeActivated>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    ui_nodes: Query<(&UiImmersiveModeActions, &UiDocumentOwner)>,
    ui_document_roots: Query<&UiDocumentRoot>,
    selected_entity: Res<SelectedEntity>,
    mut immersive_mode_entry_requests: MessageWriter<EnterImmersiveMode>,
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
            let Some(immersive_mode) = (match action_record.mode {
                UiImmersiveModeKind::SelectedEntityFirstPerson => Some(ImmersiveMode::FirstPerson),
                UiImmersiveModeKind::Cloning => None,
                UiImmersiveModeKind::FossilSearch => Some(ImmersiveMode::FossilSearch),
                UiImmersiveModeKind::FossilAssembly => Some(ImmersiveMode::FossilAssembly),
                UiImmersiveModeKind::GuestView => Some(ImmersiveMode::GuestView),
                UiImmersiveModeKind::Photo => Some(ImmersiveMode::Photo),
                UiImmersiveModeKind::SuperStaff => Some(ImmersiveMode::SuperStaff),
            }) else {
                continue;
            };
            immersive_mode_entry_requests.write(EnterImmersiveMode {
                mode: immersive_mode,
                controller: activation.node,
                subject: selected_entity.0,
            });
        }
    }
}
