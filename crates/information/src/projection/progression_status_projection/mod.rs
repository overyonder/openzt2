use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::node_property_binding::{
        UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource, UiTextPropertyBindingSource,
    },
    AssetId,
};

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::progression::award_and_progression_fact_types::EarnedProgressionAward;
use openzt2_zoo_state::progression::research_types::ResearchProject;
use openzt2_zoo_state::progression::unlock_types::UnlockedCatalogueDefinitionSet;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiVisibleBinding;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;

use super::super::projection::text_replacement_operations::replace_projected_ui_text_if_changed;

// Progression bindings combine calendar, fame, awards and rating values.
#[allow(clippy::too_many_arguments)]
pub(crate) fn project_progression_status_to_authored_information_bindings(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    unlocked_catalogue_definitions: Res<UnlockedCatalogueDefinitionSet>,
    research_projects: Query<&ResearchProject>,
    earned_progression_awards: Query<&EarnedProgressionAward>,
    mut authored_text: Query<(&UiTextBinding, &mut Text)>,
    mut authored_visibility: Query<(&UiVisibleBinding, &mut Visibility)>,
    mut authored_values: Query<(&UiValueBinding, &mut UiValue)>,
) {
    let active_research_project = research_projects.iter().next();
    let latest_earned_award = earned_progression_awards
        .iter()
        .max_by_key(|award| award.earned_tick);

    for (property_binding, mut projected_visibility) in &mut authored_visibility {
        let should_be_visible = match &property_binding.0 {
            UiBooleanPropertyBindingSource::ResearchActive => active_research_project.is_some(),
            UiBooleanPropertyBindingSource::AwardEarned { award } => earned_progression_awards
                .iter()
                .any(|earned_award| earned_award.definition == *award),
            UiBooleanPropertyBindingSource::DefinitionUnlocked { definition } => {
                active_world_definitions
                    .get(&world_definition_assets)
                    .and_then(|world_definitions| {
                        world_definitions
                            .catalogue_entry_for_definition(*definition)
                            .map(|(index, _)| index)
                    })
                    .is_some_and(|catalogue_index| {
                        catalogue_definition_is_unlocked(
                            &unlocked_catalogue_definitions,
                            catalogue_index,
                        )
                    })
            }
            _ => continue,
        };
        projected_visibility.set_if_neq(if should_be_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }

    for (property_binding, mut projected_value) in &mut authored_values {
        let value = match &property_binding.0 {
            UiIntegerPropertyBindingSource::ResearchElapsedTicks => active_research_project
                .and_then(|project| i64::try_from(project.elapsed_ticks).ok())
                .unwrap_or(0),
            UiIntegerPropertyBindingSource::ResearchRequiredTicks => active_research_project
                .and_then(|project| i64::try_from(project.required_ticks).ok())
                .unwrap_or(0),
            UiIntegerPropertyBindingSource::AwardEarnedTick { award } => earned_progression_awards
                .iter()
                .filter(|earned_award| earned_award.definition == *award)
                .map(|earned_award| earned_award.earned_tick)
                .max()
                .and_then(|earned_tick| i64::try_from(earned_tick).ok())
                .unwrap_or(0),
            _ => continue,
        };
        projected_value.set_if_neq(UiValue(value));
    }

    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(localization_catalogue) =
        active_localization.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    for (property_binding, projected_text) in &mut authored_text {
        let localization_key = match &property_binding.0 {
            UiTextPropertyBindingSource::ResearchName => active_research_project
                .and_then(|project| world_definitions.find_research(project.definition))
                .map(|record| AssetId(record.name_key.0)),
            UiTextPropertyBindingSource::LatestAwardName => latest_earned_award
                .and_then(|award| world_definitions.find_award(award.definition))
                .map(|record| AssetId(record.name_key.0)),
            _ => None,
        };
        if let Some(localized_text) =
            localization_key.and_then(|key| localization_catalogue.find_plain_localized_text(key))
        {
            replace_projected_ui_text_if_changed(projected_text, localized_text);
        }
    }
}

fn catalogue_definition_is_unlocked(
    unlocked_catalogue_definitions: &UnlockedCatalogueDefinitionSet,
    catalogue_index: usize,
) -> bool {
    unlocked_catalogue_definitions
        .words
        .get(catalogue_index / 64)
        .is_some_and(|word| word & (1_u64 << (catalogue_index % 64)) != 0)
}
