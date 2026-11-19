use bevy::prelude::*;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::{ui_document::action::UiTrigger, AssetId};
use openzt2_interface_state::ui::{
    authored_ui_activation_contracts::{ActivateAuthoredUiTool, UiNodeActivated},
    ui_document_lifecycle_contracts::ShowUiDocument,
};

#[derive(Component)]
pub(super) struct UiToolCommand(pub(super) AssetId);

#[derive(Component)]
pub(super) struct UiEditDocument(pub(super) Handle<UiDocumentAsset>);

pub(super) fn route_authored_tool_and_edit_activations(
    mut activations: MessageReader<UiNodeActivated>,
    tools: Query<&UiToolCommand>,
    editors: Query<&UiEditDocument>,
    mut tool_requests: MessageWriter<ActivateAuthoredUiTool>,
    mut open: MessageWriter<ShowUiDocument>,
) {
    for event in activations
        .read()
        .filter(|event| event.trigger == UiTrigger::Press)
    {
        if let Ok(tool) = tools.get(event.node) {
            tool_requests.write(ActivateAuthoredUiTool {
                control: event.node,
                command: tool.0,
                source: event.source,
            });
        }
        if let Ok(editor) = editors.get(event.node) {
            open.write(ShowUiDocument {
                document: editor.0.clone(),
                owner: event.node,
            });
        }
    }
}
