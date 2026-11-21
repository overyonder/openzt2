//! The selected animal's, guest's or staff member's current authored action.

use bevy::prelude::*;
use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::{ui_document::node_property_binding::UiTextPropertyBindingSource, AssetId};
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_interface_state::information::entity_selection_types::SelectedEntity;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;
use openzt2_zoo_state::behavior_task_execution_types::{BehaviorFeedbackLine, BehaviorFeedbackLog};

use super::text_replacement_operations::replace_projected_ui_text_if_changed;

pub(crate) fn project_selected_entity_current_action(
    selected_entity: Res<SelectedEntity>,
    logs: Query<&BehaviorFeedbackLog>,
    names: Query<(Option<&Name>, Option<&Inspectable>)>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    mut text_nodes: Query<(&UiTextBinding, &mut Text)>,
) {
    let Some(selected) = selected_entity.0 else {
        return;
    };
    let Some(localization) =
        active_localization.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    let name_of = |entity: Entity| -> Option<String> {
        let (name, inspectable) = names.get(entity).ok()?;
        name.map(|name| name.as_str().to_owned()).or_else(|| {
            let definition = world_definitions?.find_object(inspectable?.definition)?;
            localization
                .find_plain_localized_text(definition.name_key)
                .map(str::to_owned)
        })
    };
    let action = logs
        .get(selected)
        .ok()
        .and_then(|log| log.action.as_ref())
        .and_then(|line| {
            feedback_line_text(
                line,
                |key| localization.find_plain_localized_text(key),
                name_of,
            )
        })
        .unwrap_or_default();
    for (binding, mut text) in &mut text_nodes {
        if matches!(
            binding.0,
            UiTextPropertyBindingSource::SelectedEntityCurrentAction
        ) {
            replace_projected_ui_text_if_changed(&mut text.0, &action);
        }
    }
}

/// The localized text of a feedback line, with the named entity in its `%s`.
fn feedback_line_text<'a>(
    line: &BehaviorFeedbackLine,
    localized: impl Fn(AssetId) -> Option<&'a str>,
    name_of: impl Fn(Entity) -> Option<String>,
) -> Option<String> {
    let text = localized(AssetId::from_key(&canonicalize_source_document_record_key(
        &line.localization_key,
    )))?;
    let name = line.named.and_then(name_of).unwrap_or_default();
    Some(text.replacen("%s", &name, 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_names_its_target() {
        let line = BehaviorFeedbackLine {
            localization_key: "staffactions:GoHealAnimal".into(),
            named: Some(Entity::PLACEHOLDER),
        };
        let key = AssetId::from_key("staffactions:gohealanimal");
        let text = feedback_line_text(
            &line,
            |id| (id == key).then_some("Going to treat %s"),
            |_| Some("Lucy".into()),
        );
        assert_eq!(text.as_deref(), Some("Going to treat Lucy"));
    }
}
