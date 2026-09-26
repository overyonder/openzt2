use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::UiInformationAction;

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::ui::{
        authored_ui_action_projection_components::UiInformationActions,
        authored_ui_focus_state::UiFocusable,
        authored_ui_interaction_enabled_state::UiInteractionEnabled,
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
    },
};

use crate::plugins::information::zoopedia::zoopedia_navigation_types::ZoopediaHistory;

/// Enables Back and Forward when there is a page to visit.
pub(in crate::plugins::information) fn project_zoopedia_history_availability_to_authored_navigation_controls(
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    document_roots: Query<&UiDocumentRoot>,
    zoopedia_histories: Query<Ref<ZoopediaHistory>>,
    mut authored_navigation_controls: Query<(
        Ref<UiInformationActions>,
        &UiDocumentOwner,
        &mut UiInteractionEnabled,
        Option<&mut UiFocusable>,
        &mut Interaction,
    )>,
) {
    for (authored_actions, document_owner, mut interaction_enabled, focusable, mut interaction) in
        &mut authored_navigation_controls
    {
        let Ok(history) = zoopedia_histories.get(document_owner.0) else {
            continue;
        };
        if !history.is_changed() && !authored_actions.is_added() {
            continue;
        }
        let Some(ui_document) = document_roots
            .get(document_owner.0)
            .ok()
            .and_then(|root| ui_document_assets.get(&root.document))
        else {
            continue;
        };
        let availability = authored_actions
            .authored_action_records(ui_document)
            .find_map(|record| match &record.action {
                UiInformationAction::ZoopediaBack => Some(history.previous_subject_is_available()),
                UiInformationAction::ZoopediaForward => Some(history.next_subject_is_available()),
                _ => None,
            });
        let Some(availability) = availability else {
            continue;
        };

        interaction_enabled.0 = availability;
        if let Some(mut focusable) = focusable {
            focusable.enabled = availability;
        }
        if !availability {
            *interaction = Interaction::None;
        }
    }
}
