use bevy::prelude::*;
use openzt2_interface_state::{
    information::entity_selection_types::{
        InfoPanel, Inspectable, SelectedBy, SelectedSubject, SelectionChanged, SelectionRequest,
    },
    ui::{
        authored_ui_node_projection_components::UiDocumentRoot,
        ui_document_lifecycle_contracts::{InGameUiOwner, ShowUiRole},
    },
};
use openzt2_world_state::world_spawn::world_entity_crating::ContainedCratedAnimal;

pub(super) fn apply_world_ui_selection(
    mut commands: Commands,
    mut requests: MessageReader<SelectionRequest>,
    owners: Query<(Entity, Option<&SelectedSubject>), With<InGameUiOwner>>,
    inspectable: Query<(), With<Inspectable>>,
    containers: Query<&ContainedCratedAnimal>,
    mut changed: MessageWriter<SelectionChanged>,
) {
    let Ok((owner, current)) = owners.single() else {
        requests.clear();
        return;
    };
    let previous = current.map(|selection| selection.0);
    let mut current = previous;
    for request in requests.read() {
        let subject = request.entity.map(|entity| {
            containers
                .get(entity)
                .map_or(entity, |contained| contained.0)
        });
        if subject.is_none_or(|entity| inspectable.contains(entity)) {
            current = subject;
        }
    }
    if current == previous {
        return;
    }
    if let Some(subject) = current {
        commands.entity(owner).insert(SelectedSubject(subject));
    } else {
        commands.entity(owner).remove::<SelectedSubject>();
    }
    changed.write(SelectionChanged { previous, current });
}

pub(super) fn clear_world_ui_selection_on_loss_of_inspectability(
    removed: On<Remove<Inspectable>>,
    selectors: Query<&SelectedBy>,
    owners: Query<(), With<InGameUiOwner>>,
    mut commands: Commands,
    mut changed: MessageWriter<SelectionChanged>,
) {
    let Ok(selectors) = selectors.get(removed.entity) else {
        return;
    };
    for owner in selectors.iter().filter(|owner| owners.contains(*owner)) {
        commands.entity(owner).try_remove::<SelectedSubject>();
        changed.write(SelectionChanged {
            previous: Some(removed.entity),
            current: None,
        });
    }
}

pub(super) fn show_information_for_world_ui_selection(
    owners: Query<
        (Entity, Option<Ref<SelectedSubject>>, Option<Ref<Children>>),
        With<InGameUiOwner>,
    >,
    inspectable: Query<&Inspectable>,
    definitions: Res<openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions>,
    assets: Res<Assets<openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset>>,
    mut selection_changed: MessageReader<SelectionChanged>,
    mut panels: Query<&mut Visibility, (With<UiDocumentRoot>, With<InfoPanel>)>,
    mut show: MessageWriter<ShowUiRole>,
) {
    use openzt2_game_data::{
        ui_document::document::UiDocumentRole,
        world_definitions::world_objects::WorldObjectInformationViewClass,
    };
    let selection_changed = selection_changed.read().count() != 0;
    let catalog_changed = definitions.is_changed() || assets.is_changed();
    for (owner, selected, children) in &owners {
        if !selection_changed
            && !catalog_changed
            && !selected
                .as_ref()
                .is_some_and(|selection| selection.is_changed())
            && !children
                .as_ref()
                .is_some_and(|children| children.is_changed())
        {
            continue;
        }
        let show_panel = selected
            .and_then(|subject| inspectable.get(subject.0).ok())
            .is_some_and(|subject| {
                !definitions
                    .get(&assets)
                    .and_then(|definitions| definitions.find_object(subject.definition))
                    .is_some_and(|definition| {
                        matches!(
                            definition.view_class,
                            Some(WorldObjectInformationViewClass::Entrance)
                        )
                    })
            });
        let panel = children
            .as_ref()
            .into_iter()
            .flat_map(|children| children.iter())
            .find(|child| panels.contains(*child));
        if let Some(panel) = panel {
            if let Ok(mut visibility) = panels.get_mut(panel) {
                visibility.set_if_neq(if show_panel {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        } else if show_panel {
            show.write(ShowUiRole {
                role: UiDocumentRole::EntityInfo,
                owner,
            });
        }
    }
}

#[cfg(test)]
mod tests;
