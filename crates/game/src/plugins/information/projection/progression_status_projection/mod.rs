use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::node_property_binding::{
        UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource, UiTextPropertyBindingSource,
    },
    AssetId,
};

use crate::assets::localization::localization_asset_types::LocalizationAsset;
use crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::progression::award_and_progression_fact_types::EarnedProgressionAward;
use crate::plugins::progression::fame_types::Fame;
use crate::plugins::progression::rating_types::ZooRating;
use crate::plugins::progression::research_types::ResearchProject;
use crate::plugins::progression::unlock_types::UnlockedCatalogueDefinitionSet;
use crate::plugins::ui::authored_ui_node_projection_components::UiValue;
use crate::plugins::ui::authored_ui_node_projection_components::UiValueBinding;
use crate::plugins::ui::authored_ui_node_projection_components::UiVisibleBinding;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;

use super::super::projection::text_replacement_operations::replace_projected_ui_text_if_changed;

// Progression bindings combine calendar, fame, awards and rating values.
#[allow(clippy::too_many_arguments)]
pub(in crate::plugins::information) fn project_progression_status_to_authored_information_bindings(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    zoo_fame: Res<Fame>,
    zoo_rating: Res<ZooRating>,
    unlocked_catalogue_definitions: Res<UnlockedCatalogueDefinitionSet>,
    research_projects: Query<&ResearchProject>,
    earned_progression_awards: Query<&EarnedProgressionAward>,
    mut authored_text: Query<(&UiTextBinding, &mut Text)>,
    mut authored_values_and_visibility: ParamSet<(
        Query<(&UiVisibleBinding, &mut Visibility)>,
        Query<(&UiValueBinding, &mut UiValue, &mut Visibility)>,
    )>,
) {
    let active_research_project = research_projects.iter().next();
    let latest_earned_award = earned_progression_awards
        .iter()
        .max_by_key(|award| award.earned_tick);

    for (property_binding, mut projected_visibility) in &mut authored_values_and_visibility.p0() {
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
                            .catalogue()
                            .position(|entry| entry.definition.0 == definition.0)
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
        *projected_visibility = if should_be_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (property_binding, mut projected_value, mut visibility) in
        &mut authored_values_and_visibility.p1()
    {
        projected_value.0 = match &property_binding.0 {
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
            UiIntegerPropertyBindingSource::ZooFameHalfStars => i64::from(zoo_fame.half_stars),
            UiIntegerPropertyBindingSource::ZooRatingPermille => {
                visibility.set_if_neq(if zoo_rating.overall_available {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
                if !zoo_rating.overall_available {
                    continue;
                }
                i64::from(zoo_rating.overall_permille)
            }
            _ => continue,
        };
    }

    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(localization_catalogue) =
        active_localization.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    for (property_binding, mut projected_text) in &mut authored_text {
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
            replace_projected_ui_text_if_changed(&mut projected_text.0, localized_text);
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
