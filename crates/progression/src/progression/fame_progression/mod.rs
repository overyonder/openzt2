use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow},
    prelude::*,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::{
    lua_script::{lua_script_archive_path_index::LuaScriptArchivePathIndex, lua_script_asset_loading::LuaScriptAsset},
    localization::{localization_asset_types::LocalizationAsset, localization_precedence_index::LocalizationPrecedenceIndex},
};
use openzt2_game_data::AssetId;
use openzt2_interface_state::information::zoo_message_types::{ZooMessage, ZooMessages};
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted,
    world_membership_types::{DefinitionId, WorldMember, WorldRoot},
    zoo_entrance_anchor_synchronization::ZooEntrance,
};
use openzt2_zoo_state::progression::{
    fame_types::{FameAuthoredPresentation, FameGateAppearanceRequested},
    profile_scenario_variables::ProfileScenarioVariables,
};
use super::authored_fame_script::{entrance_gate_definitions, AuthoredFameScript, FameScriptEffect};

use super::{
    award_and_progression_fact_types::{ProgressionFactChanged, ScenarioAwardPointTotal},
    catalogue_definition_index_queries::find_catalogue_definition_index,
    catalogue_unlock_set_operations::{
        catalogue_definition_is_unlocked_at_index, mark_catalogue_definition_unlocked_at_index,
    },
    fame_calculations::{
        select_fame_level_for_rating_and_guest_count, update_current_and_maximum_fame,
    },
    fame_types::{Fame, ZooMaximumFame},
    profile_challenge_types::TotalAnimalsReleasedCount,
    rating_types::ZooRating,
    unlock_types::UnlockedCatalogueDefinitionSet,
    zoo_rating_input_types::{EarnedAwardFamePoints, GuestRatingFacts, LivingAnimalPopulation},
};

pub(super) fn apply_scenario_maximum_fame_requests(
    mut requests: MessageReader<openzt2_zoo_state::progression::fame_types::SetScenarioMaximumFame>,
    mut worlds: Query<&mut Fame, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    for request in requests.read() {
        let Ok(mut fame) = worlds.single_mut() else {
            continue;
        };
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
    scenario_award_points: Res<ScenarioAwardPointTotal>,
    total_animals_released: Res<TotalAnimalsReleasedCount>,
    script_assets: Res<Assets<LuaScriptAsset>>,
    script_index: Res<LuaScriptArchivePathIndex>,
    active_world: Query<Entity, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    mut worlds: Query<
        (
            Entity,
            &mut Fame,
            &LivingAnimalPopulation,
            &GuestRatingFacts,
            &EarnedAwardFamePoints,
            &ZooRating,
            Option<(&mut AuthoredFameScript, &mut FameAuthoredPresentation)>,
        ),
        With<WorldRoot>,
    >,
    gates: Query<
        (
            Entity,
            &WorldMember,
            &DefinitionId,
            Option<&openzt2_zoo_state::progression::fame_types::PendingFameGatePrefab>,
        ),
        (With<ZooEntrance>, Allow<Disabled>),
    >,
    mut presentation: FamePresentation,
    mut unlocks: ResMut<UnlockedCatalogueDefinitionSet>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let active_world = active_world.single().ok();
    for (world, mut world_fame, animals, guests, awards, rating, script) in &mut worlds {
        let active = active_world == Some(world);
        let mut fame_written = false;
        let mut next_fame = *world_fame;
        if let Some((mut script, mut authored_ui)) = script {
            let gates = entrance_gate_definitions(world, definitions, &gates);
            let result = script.evaluate(
                &script_index,
                &script_assets,
                animals,
                guests,
                next_fame,
                awards.0,
                if active { scenario_award_points.0 } else { 0 },
                if active { total_animals_released.0 } else { 0 },
                &gates,
                presentation.variables.as_deref(),
            );
            let result = match result {
                Ok(result) => result,
                Err(error) => {
                    debug!(%error, "authored fame evaluation failed");
                    continue;
                }
            };

            for effect in result.effects {
                match effect {
                    FameScriptEffect::Fame(half_stars, percent) => {
                        update_current_and_maximum_fame(&mut next_fame, half_stars, percent);
                        fame_written = true;
                        if world_fame.set_if_neq(next_fame) && active {
                            changed.write(ProgressionFactChanged::Fame);
                        }
                    }
                    FameScriptEffect::Gate(request) => {
                        presentation.gates.write(request);
                    }
                    FameScriptEffect::Notice(key, seconds) if active => {
                        let view = presentation
                            .active_localization
                            .as_deref()
                            .zip(presentation.localizations.as_deref())
                            .and_then(|(index, assets)| {
                                index.borrow_loaded_localization_view(assets)
                            });
                        if let (Some(view), Some(messages), Some(now)) = (
                            view,
                            presentation.messages.as_deref_mut(),
                            presentation.now.as_deref(),
                        ) {
                            if let Some(text) =
                                view.find_plain_localized_text(AssetId::from_key(&key))
                            {
                                messages.messages.push(ZooMessage {
                                    text: text.to_owned(),
                                    expires_at_seconds: now.elapsed_secs_f64() + seconds,
                                });
                            }
                        }
                    }
                    FameScriptEffect::Notice(_, _) => {}
                }
            }
            authored_ui.set_if_neq(result.presentation);
        } else if rating.overall_available && definitions.fame_thresholds().next().is_some() {
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
            update_current_and_maximum_fame(&mut next_fame, next, f32::from(next) * 10.0);
            fame_written = true;
            if world_fame.set_if_neq(next_fame) && active {
                changed.write(ProgressionFactChanged::Fame);
            }
        }

        if !active || !fame_written {
            continue;
        }
        let next_half_stars = next_fame.half_stars;
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
                    if mark_catalogue_definition_unlocked_at_index(&mut unlocks, index)
                        == Some(true)
                    {
                        changed.write(ProgressionFactChanged::Unlock(id));
                    }
                }
            }
        }
    }
}

pub(super) fn publish_world_maximum_fame(
    mut worlds: Query<(&Fame, &mut ZooMaximumFame), (With<WorldRoot>, Changed<Fame>)>,
) {
    for (fame, mut maximum) in &mut worlds {
        maximum.set_if_neq(ZooMaximumFame(fame.maximum_percent_reached));
    }
}
