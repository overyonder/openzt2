use bevy::prelude::*;
use openzt2_game_data::ui_document::action::{
    information::{InformationViewCategory, UiInformationAction},
    UiTrigger,
};

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiInformationActions;
use openzt2_interface_state::ui::authored_ui_node_projection_components::{
    UiDocumentOwner, UiDocumentRoot,
};
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;
use openzt2_world_state::world_spawn::world_entity_view_filter::HiddenByWorldViewFilter;

use super::information_view_types::{InformationViewClass, InformationViewFilters};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WorldViewFilterToggle {
    category: InformationViewCategory,
    selected_when_visible: bool,
}

pub(super) fn set_information_view_category_visibility_from_authored_action(
    category: InformationViewCategory,
    visible: bool,
    filters: &mut ResMut<InformationViewFilters>,
) {
    if filters.category_is_visible(category) != visible {
        filters.set_category_visibility(category, visible);
    }
}

pub(super) fn hydrate_authored_world_view_filter_toggle_bindings(
    mut commands: Commands,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    mut controls: Query<
        (
            Entity,
            Ref<UiInformationActions>,
            Ref<UiDocumentOwner>,
            Ref<UiSelected>,
            Option<&mut WorldViewFilterToggle>,
        ),
        With<UiSelected>,
    >,
) {
    for (node, actions, owner, selection, current) in &mut controls {
        if !documents.is_changed()
            && !actions.is_changed()
            && !owner.is_changed()
            && !selection.is_added()
        {
            continue;
        }
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
        else {
            continue;
        };
        let binding = actions
            .authored_action_records(document)
            .find_map(|record| {
                let UiInformationAction::SetViewFilter { category, visible } = &record.action
                else {
                    return None;
                };
                let selected_when_visible = match record.trigger {
                    UiTrigger::On => *visible,
                    UiTrigger::Off => !visible,
                    _ => return None,
                };
                Some(WorldViewFilterToggle {
                    category: *category,
                    selected_when_visible,
                })
            });
        match (current, binding) {
            (Some(mut current), Some(binding)) => {
                current.set_if_neq(binding);
            }
            (None, Some(binding)) => {
                commands.entity(node).insert(binding);
            }
            (Some(_), None) => {
                commands.entity(node).remove::<WorldViewFilterToggle>();
            }
            (None, None) => {}
        }
    }
}

pub(super) fn project_information_view_filters_to_world_entities(
    mut commands: Commands,
    filters: Res<InformationViewFilters>,
    all_objects: Query<(Entity, &InformationViewClass, Has<HiddenByWorldViewFilter>)>,
    changed_objects: Query<
        (Entity, &InformationViewClass, Has<HiddenByWorldViewFilter>),
        Changed<InformationViewClass>,
    >,
    mut removed_classes: RemovedComponents<InformationViewClass>,
    hidden: Query<(), With<HiddenByWorldViewFilter>>,
) {
    let filters_changed = filters.is_changed();
    let affected = filters_changed
        .then(|| all_objects.iter())
        .into_iter()
        .flatten()
        .chain(
            (!filters_changed)
                .then(|| changed_objects.iter())
                .into_iter()
                .flatten(),
        );
    for (entity, class, hidden) in affected {
        let visible = filters.category_is_visible(class.0);
        if visible && hidden {
            commands.entity(entity).remove::<HiddenByWorldViewFilter>();
        } else if !visible && !hidden {
            commands.entity(entity).insert(HiddenByWorldViewFilter);
        }
    }
    for entity in removed_classes.read() {
        if !all_objects.contains(entity) && hidden.contains(entity) {
            commands.entity(entity).remove::<HiddenByWorldViewFilter>();
        }
    }
}

pub(super) fn project_information_view_filters_to_authored_controls(
    filters: Res<InformationViewFilters>,
    mut selections: Query<(Ref<WorldViewFilterToggle>, &mut UiSelected)>,
) {
    let filters_changed = filters.is_changed();
    for (binding, mut selection) in &mut selections {
        if filters_changed || binding.is_changed() || selection.is_changed() {
            let visible = filters.category_is_visible(binding.category);
            selection.set_if_neq(UiSelected(visible == binding.selected_when_visible));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_toggles_preserve_an_authored_hidden_entity() {
        let mut app = App::new();
        app.init_resource::<InformationViewFilters>()
            .add_systems(Update, project_information_view_filters_to_world_entities);
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
        let newcomer = app
            .world_mut()
            .spawn(InformationViewClass(InformationViewCategory::Animals))
            .id();
        app.update();
        assert!(app
            .world()
            .get::<HiddenByWorldViewFilter>(newcomer)
            .is_some());
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
