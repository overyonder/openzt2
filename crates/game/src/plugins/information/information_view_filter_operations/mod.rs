use bevy::prelude::*;
use openzt2_game_data::ui_document::action::{
    information::{InformationViewCategory, UiInformationAction},
    UiTrigger,
};

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::ui::{
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiDocumentRoot,
        authored_ui_selection_state::UiSelected,
    },
};

use super::information_view_types::{InformationViewClass, InformationViewFilters};
use crate::plugins::ui::authored_ui_action_projection_components::UiInformationActions;

pub(super) fn set_information_view_category_visibility_from_authored_action(
    category: InformationViewCategory,
    visible: bool,
    filters: &mut InformationViewFilters,
) {
    filters.set_category_visibility(category, visible);
}

/// Projects one canonical view-filter policy to both live world visibility and
/// authored toggle selection.
pub(super) fn project_information_view_filters_to_world_entities_and_authored_controls(
    filters: Res<InformationViewFilters>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    document_roots: Query<&UiDocumentRoot>,
    mut classified_world_entities: Query<(&InformationViewClass, &mut Visibility)>,
    mut authored_controls: Query<(&UiInformationActions, &UiDocumentOwner, &mut UiSelected)>,
) {
    for (information_view_class, mut visibility) in &mut classified_world_entities {
        let projected_visibility = if filters.category_is_visible(information_view_class.0) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != projected_visibility {
            *visibility = projected_visibility;
        }
    }

    for (authored_actions, document_owner, mut selected) in &mut authored_controls {
        let Ok(document_root) = document_roots.get(document_owner.0) else {
            continue;
        };
        let Some(ui_document_asset) = ui_document_assets.get(&document_root.document) else {
            continue;
        };
        let mut authored_action_records =
            authored_actions.authored_action_records(ui_document_asset);
        let projected_selection = authored_action_records.find_map(|record| {
            let UiInformationAction::SetViewFilter { category, visible } = &record.action else {
                return None;
            };
            let category_is_visible = filters.category_is_visible(*category);
            match record.trigger {
                UiTrigger::On => Some(category_is_visible == *visible),
                UiTrigger::Off => Some(category_is_visible != *visible),
                _ => None,
            }
        });
        if let Some(projected_selection) = projected_selection.filter(|value| selected.0 != *value)
        {
            selected.0 = projected_selection;
        }
    }
}
