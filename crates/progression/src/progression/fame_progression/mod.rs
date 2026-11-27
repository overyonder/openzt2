use bevy::{ecs::entity::EntityHashMap, prelude::*};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::{
    lua_script::{lua_script_sources::LuaScriptSources, lua_script_asset_loading::LuaScriptAsset},
    localization::{localization_asset_types::LocalizationAsset, localization_precedence_index::LocalizationPrecedenceIndex},
};
use openzt2_game_data::AssetId;
use openzt2_interface_state::information::zoo_message_types::{ZooMessage, ZooMessages};
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted,
    world_membership_types::{WorldMember, WorldMembers, WorldRoot},
};
use openzt2_zoo_state::progression::{
    fame_types::{FameAuthoredPresentation, FameGateAppearanceRequested, ZooSpeciesFameViews},
    profile_scenario_variables::ProfileScenarioVariables,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_trigger_types::BehaviorSubjectTrigger,
    guests::guest_simulation_types::Guest,
};
use super::authored_fame_script::{AuthoredFameScript, FameEntityFacts, FameScriptEffect};

use super::{
    award_and_progression_fact_types::ProgressionFactChanged,
    fame_calculations::{
        select_fame_level_for_rating_and_guest_count, update_current_and_maximum_fame,
    },
    fame_types::{Fame, ZooMaximumFame},
    profile_challenge_types::TotalAnimalsReleasedCount,
    rating_types::ZooRating,
    zoo_rating_input_types::GuestRatingFacts,
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
pub(super) struct FamePresentation<'w, 's> {
    active_localization: Option<Res<'w, LocalizationPrecedenceIndex>>,
    localizations: Option<Res<'w, Assets<LocalizationAsset>>>,
    messages: Query<'w, 's, &'static mut ZooMessages>,
    now: Option<Res<'w, Time<Real>>>,
    gates: MessageWriter<'w, FameGateAppearanceRequested>,
    variables: Option<Res<'w, ProfileScenarioVariables>>,
}

pub(super) fn update_authored_fame<const DEPARTURES_ONLY: bool>(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,

    script_assets: Res<Assets<LuaScriptAsset>>,
    script_sources: Res<LuaScriptSources>,
    active_world: Query<Entity, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    mut worlds: Query<
        (
            Entity,
            &mut Fame,
            Option<&WorldMembers>,
            &GuestRatingFacts,
            Option<&ZooSpeciesFameViews>,
            &super::authored_award_programs::AuthoredAwardPoints,
            Option<&super::AwardGlobalVariables>,
            &ZooRating,
            &TotalAnimalsReleasedCount,
            Option<(&mut AuthoredFameScript, &mut FameAuthoredPresentation)>,
        ),
        With<WorldRoot>,
    >,
    entities: FameEntityFacts,
    mut presentation: FamePresentation,
    mut changed: MessageWriter<ProgressionFactChanged>,
    mut subject_triggers: Option<MessageReader<BehaviorSubjectTrigger>>,
    guest_worlds: Query<&WorldMember, With<Guest>>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let active_world = active_world.single().ok();
    let mut departing = EntityHashMap::<usize>::default();
    if let Some(triggers) = subject_triggers.as_mut().filter(|_| DEPARTURES_ONLY) {
        let adjust_fame = AssetId::from_key("adjust_zoofame");
        for trigger in triggers
            .read()
            .filter(|trigger| trigger.trigger == adjust_fame)
        {
            if let Ok(member) = guest_worlds.get(trigger.subject) {
                *departing.entry(member.root).or_default() += 1;
            }
        }
    }
    if DEPARTURES_ONLY && departing.is_empty() {
        return;
    }
    for (
        world,
        mut world_fame,
        members,
        guests,
        views,
        awards,
        award_globals,
        rating,
        total_animals_released,
        mut script,
    ) in &mut worlds
    {
        let active = active_world == Some(world);
        let evaluations = if DEPARTURES_ONLY {
            departing.remove(&world).unwrap_or_default()
        } else {
            1
        };
        for _ in 0..evaluations {
            let mut next_fame = *world_fame;
            if let Some((script, authored_ui)) = script.as_mut() {
                let result = script.evaluate(
                    &script_sources,
                    &script_assets,
                    views,
                    next_fame,
                    awards.0,
                    total_animals_released.0,
                    &entities,
                    members,
                    definitions,
                    presentation.variables.as_deref(),
                    award_globals,
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
                            if let (Some(view), Ok(mut messages), Some(now)) = (
                                view,
                                presentation.messages.get_mut(world),
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
                if world_fame.set_if_neq(next_fame) && active {
                    changed.write(ProgressionFactChanged::Fame);
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
