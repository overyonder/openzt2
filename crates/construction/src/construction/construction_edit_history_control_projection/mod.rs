use super::{
    construction_history::ConstructionHistory, construction_lifecycle::ConstructionActive,
};
use bevy::prelude::*;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::action::construction::UiConstructionAction;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiConstructionActions;
use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionEditDirection;

pub(super) fn project_construction_undo_availability(
    history: ConstructionHistory,
    pending_transactions: Query<(), With<ConstructionActive>>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    mut controls: Query<(
        &UiConstructionActions,
        &UiDocumentOwner,
        &mut UiInteractionEnabled,
    )>,
) {
    let undo_available =
        history.next(ConstructionEditDirection::Undo).is_some() && pending_transactions.is_empty();

    for (actions, owner, mut enabled) in &mut controls {
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
        else {
            continue;
        };
        if !actions
            .authored_action_records(document)
            .any(|record| matches!(&record.action, UiConstructionAction::Undo))
        {
            continue;
        }
        if enabled.0 != undo_available {
            enabled.0 = undo_available;
        }
    }
}
