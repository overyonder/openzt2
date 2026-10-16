use bevy::prelude::*;
use openzt2_game_data::ui_document::action::{
    information::{InformationViewCategory, UiInformationAction},
    UiTrigger,
};

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;

use super::information_view_types::{InformationViewClass, InformationViewFilters};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiInformationActions;
use openzt2_world_state::world_spawn::world_entity_view_filter::HiddenByWorldViewFilter;

pub(super) fn set_information_view_category_visibility_from_authored_action(
    category: InformationViewCategory,
    visible: bool,
    filters: &mut InformationViewFilters,
) {
    filters.set_category_visibility(category, visible);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_toggles_preserve_an_authored_hidden_entity() {
        let mut app = App::new();
        app.init_resource::<InformationViewFilters>()
            .init_resource::<Assets<UiDocumentAsset>>()
            .add_systems(
                Update,
                project_information_view_filters_to_world_entities_and_authored_controls,
            );
        let animal = app
            .world_mut()
            .spawn((
                InformationViewClass(InformationViewCategory::Animals),
                Visibility::Hidden,
            ))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(animal),
            Some(&Visibility::Hidden)
        );
        app.world_mut()
            .resource_mut::<InformationViewFilters>()
            .set_category_visibility(InformationViewCategory::Animals, false);
        app.update();
        assert!(app.world().get::<HiddenByWorldViewFilter>(animal).is_some());
        app.world_mut()
            .resource_mut::<InformationViewFilters>()
            .set_category_visibility(InformationViewCategory::Animals, true);
        app.update();
        assert!(app.world().get::<HiddenByWorldViewFilter>(animal).is_none());
        assert_eq!(
            app.world().get::<Visibility>(animal),
            Some(&Visibility::Hidden)
        );
    }
}

/// Projects one canonical view-filter policy to both live world visibility and
/// authored toggle selection.
pub(super) fn project_information_view_filters_to_world_entities_and_authored_controls(
    mut commands: Commands,
    filters: Res<InformationViewFilters>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    document_roots: Query<&UiDocumentRoot>,
    classified_world_entities: Query<(Entity, &InformationViewClass, Has<HiddenByWorldViewFilter>)>,
    mut authored_controls: Query<(&UiInformationActions, &UiDocumentOwner, &mut UiSelected)>,
) {
    for (entity, information_view_class, hidden) in &classified_world_entities {
        let visible = filters.category_is_visible(information_view_class.0);
        if visible && hidden {
            commands.entity(entity).remove::<HiddenByWorldViewFilter>();
        } else if !visible && !hidden {
            commands.entity(entity).insert(HiddenByWorldViewFilter);
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
