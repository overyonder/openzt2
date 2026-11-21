use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::{
    lua_script::{lua_script_archive_path_index::LuaScriptArchivePathIndex, lua_script_asset_loading::LuaScriptAsset},
    localization::{localization_asset_types::LocalizationAsset, localization_precedence_index::LocalizationPrecedenceIndex},
};
use openzt2_game_data::AssetId;
use openzt2_interface_state::information::zoo_message_types::{ZooMessage, ZooMessages};
use openzt2_world_state::world_spawn::{
    world_membership_types::{DefinitionId, WorldMember, WorldRoot},
    zoo_entrance_anchor_synchronization::ZooEntrance,
};
use openzt2_zoo_state::progression::{
    fame_types::{FameAuthoredPresentation, FameGateAppearanceRequested},
    profile_scenario_variables::ProfileScenarioVariables,
};
use super::authored_fame_script::{entrance_gate_definitions, AuthoredFameScript};

use super::{
    award_and_progression_fact_types::{
        EarnedProgressionAward, ProgressionFactChanged, ScenarioAwardPointTotal,
    },
    catalogue_definition_index_queries::find_catalogue_definition_index,
    catalogue_unlock_set_operations::{
        catalogue_definition_is_unlocked_at_index, mark_catalogue_definition_unlocked_at_index,
    },
    fame_calculations::{
        select_fame_level_for_rating_and_guest_count, update_current_and_maximum_fame,
    },
    fame_types::Fame,
    profile_challenge_types::TotalAnimalsReleasedCount,
    rating_types::ZooRating,
    unlock_types::UnlockedCatalogueDefinitionSet,
    zoo_rating_input_types::{EarnedAwardFamePoints, GuestRatingFacts, LivingAnimalPopulation},
};

pub(super) fn apply_scenario_maximum_fame_requests(
    mut requests: MessageReader<openzt2_zoo_state::progression::fame_types::SetScenarioMaximumFame>,
    mut fame: ResMut<Fame>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    for request in requests.read() {
        if !request.percent.is_finite() || !(0.0..=100.0).contains(&request.percent) {
            continue;
        }
        if fame
            .maximum_percent_reached
            .is_none_or(|previous| request.percent > previous)
        {
            fame.maximum_percent_reached = Some(request.percent);
            fame.maximum_reached = fame
                .maximum_reached
                .max(Fame::half_stars_for_percent(request.percent));
            changed.write(ProgressionFactChanged::Fame);
        }
    }
}

pub(super) fn count_earned_award_fame_points(
    awards: Query<(), With<EarnedProgressionAward>>,
    added: Query<(), Added<EarnedProgressionAward>>,
    mut removed: RemovedComponents<EarnedProgressionAward>,
    mut points: ResMut<EarnedAwardFamePoints>,
) {
    let removed_count = removed.read().count();
    if points.is_added() || !added.is_empty() || removed_count != 0 {
        points.set_if_neq(EarnedAwardFamePoints(
            u32::try_from(awards.iter().len()).unwrap_or(u32::MAX),
        ));
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct FamePresentation<'w> {
    active_localization: Option<Res<'w, LocalizationPrecedenceIndex>>,
    localizations: Option<Res<'w, Assets<LocalizationAsset>>>,
    messages: Option<ResMut<'w, ZooMessages>>,
    now: Option<Res<'w, Time<Real>>>,
    gates: MessageWriter<'w, FameGateAppearanceRequested>,
    variables: Option<Res<'w, ProfileScenarioVariables>>,
}

pub(super) fn update_fame_and_apply_authored_fame_unlocks(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    rating: Res<ZooRating>,
    guests: Res<GuestRatingFacts>,
    scenario_award_points: Res<ScenarioAwardPointTotal>,
    awards: Res<EarnedAwardFamePoints>,
    animals: Res<LivingAnimalPopulation>,
    total_animals_released: Res<TotalAnimalsReleasedCount>,
    script_assets: Res<Assets<LuaScriptAsset>>,
    script_index: Res<LuaScriptArchivePathIndex>,
    mut worlds: Query<
        (
            Entity,
            &mut AuthoredFameScript,
            &mut FameAuthoredPresentation,
        ),
        With<WorldRoot>,
    >,
    gates: Query<(Entity, &WorldMember, &DefinitionId), With<ZooEntrance>>,
    mut presentation: FamePresentation,
    mut fame: ResMut<Fame>,
    mut unlocks: ResMut<UnlockedCatalogueDefinitionSet>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let (next_half_stars, current_percent) = if let Some((world, mut script, mut authored_ui)) =
        worlds.iter_mut().next()
    {
        let gates = entrance_gate_definitions(world, &gates);
        let result = script.evaluate(
            &script_index,
            &script_assets,
            &animals,
            &guests,
            *fame,
            awards.0,
            scenario_award_points.0,
            total_animals_released.0,
            &gates,
            presentation.variables.as_deref(),
        );
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                debug!(%error, "authored fame evaluation failed");
                return;
            }
        };
        if *authored_ui != result.presentation {
            *authored_ui = result.presentation;
        }
        for gate in result.gate_changes {
            presentation.gates.write(gate);
        }
        if !result.notices.is_empty() {
            let view = presentation
                .active_localization
                .as_deref()
                .zip(presentation.localizations.as_deref())
                .and_then(|(index, assets)| index.borrow_loaded_localization_view(assets));
            if let (Some(view), Some(messages), Some(now)) = (
                view,
                presentation.messages.as_deref_mut(),
                presentation.now.as_deref(),
            ) {
                for (key, seconds) in result.notices {
                    if let Some(text) = view.find_plain_localized_text(AssetId::from_key(&key)) {
                        messages.messages.push(ZooMessage {
                            text: text.to_owned(),
                            expires_at_seconds: now.elapsed_secs_f64() + seconds,
                        });
                    }
                }
            }
        }
        let Some(fame) = result.fame else { return };
        fame
    } else {
        if definitions.fame_thresholds().next().is_none() {
            return;
        }
        if !rating.overall_available {
            return;
        }
        let next = select_fame_level_for_rating_and_guest_count(
            definitions.fame_thresholds().map(|threshold| {
                (
                    threshold.level,
                    threshold.minimum_rating,
                    threshold.minimum_guests,
                )
            }),
            rating.overall_permille,
            guests.count,
        );
        (next, f32::from(next) * 10.0)
    };

    let mut next_fame = *fame;
    if update_current_and_maximum_fame(&mut next_fame, next_half_stars, current_percent) {
        *fame = next_fame;
        changed.write(ProgressionFactChanged::Fame);
    }
    for threshold in definitions
        .fame_thresholds()
        .filter(|threshold| threshold.level <= u16::from(next_half_stars))
    {
        let targets = &threshold.unlocks;
        if !targets
            .iter()
            .all(|target| find_catalogue_definition_index(definitions, *target).is_some())
        {
            continue;
        }
        for target in targets {
            let id = *target;
            if let Some(index) = find_catalogue_definition_index(definitions, id) {
                if catalogue_definition_is_unlocked_at_index(&unlocks, index) {
                    continue;
                }
                if mark_catalogue_definition_unlocked_at_index(&mut unlocks, index) == Some(true) {
                    changed.write(ProgressionFactChanged::Unlock(id));
                }
            }
        }
    }
}
