use super::show_scoring::scaled_popularity;
use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_assets::{
    behavior::behavior_asset_types::{BehaviorDocumentAsset, LoadedBehaviorDocumentCollection},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{AssetId, behavior::action::watch_show::ShowEventScoring};
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    animal_behavior::{
        behavior_set_start_request_types::PendingBehaviorSet,
        behavior_token_types::ClaimedBehaviorSelfToken,
    },
    animal_lifecycle::types::{Animal, SpeciesHandle},
    behavior_task_execution_types::{
        BehaviorTaskExecutionPhase, BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
        running_actions::{RunBehaviorWatchShow, RunningBehaviorInstruction, SelectedBehaviorSet},
    },
    guests::{
        guest_simulation_types::{GuestArchetype, GuestFavouriteAnimal, GuestRng},
        guest_survey_types::GuestSurveyExperience,
    },
    shows::running_show_types::{
        AnimalPerformingShowTrick, RunningShow, RunningShowOnStage, RunningShowTrick,
        ScheduledShowTrickScored, ShowAudienceReaction, ShowPhase, StageRunningShow,
        WorldTrickPopularity,
    },
};

use super::{
    authored_show_rewards::{AuthoredShowRewards, AuthoredShowRewardsSource},
    running_show_schedule::ActiveShowTrickOf,
    scheduled_trick_execution::{ScheduledTrickToken, ShowTrickTokens},
    show_audience_types::{GuestShowExperience, GuestWatchingStage, StageShowAudience},
    show_event_types::AuthoredAnimalShowEvent,
};

#[derive(Component)]
#[relationship(relationship_target = RunShowEvents)]
pub(crate) struct ShowEventForRun(Entity);

#[derive(Component)]
#[relationship_target(relationship = ShowEventForRun, linked_spawn)]
pub(crate) struct RunShowEvents(Vec<Entity>);

#[derive(Component)]
pub(crate) struct ShowEventWindow {
    release: u64,
    expires: Option<u64>,
    trick: AssetId,
}

#[derive(Component)]
#[relationship(relationship_target = ReleasedShowEventsForRun)]
pub(super) struct ReleasedShowEvent(Entity);

#[derive(Component)]
#[relationship_target(relationship = ReleasedShowEvent)]
pub(super) struct ReleasedShowEventsForRun(Vec<Entity>);

#[derive(Component)]
pub(super) struct PendingShowReaction;

pub(super) fn schedule_authored_animal_show_events(
    mut commands: Commands,
    clock: Single<&ZooClock>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    events: Query<(Entity, &AuthoredAnimalShowEvent), Without<ShowEventWindow>>,
    performers: Query<&AnimalPerformingShowTrick, With<Animal>>,
    tricks: Query<(&ActiveShowTrickOf, &RunningShowTrick, &ShowTrickTokens)>,
    shows: Query<(&WorldMember, &RunningShow)>,
    popularity: Query<&WorldTrickPopularity>,
    tokens: Query<(&ScheduledTrickToken, &ClaimedBehaviorSelfToken, &ChildOf)>,
    reward_source: Res<AuthoredShowRewardsSource>,
    rewards: Res<Assets<AuthoredShowRewards>>,
    mut scores: MessageWriter<ScheduledShowTrickScored>,
) {
    if events.is_empty() {
        return;
    }
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let ticks = |seconds: f32| {
        (f64::from(seconds) * f64::from(definitions.timing().fixed_hz)).ceil() as u64
    };
    for (entity, event) in &events {
        let Ok(active) = performers.get(event.performer) else {
            commands.entity(entity).try_despawn();
            continue;
        };
        let Ok((run, trick, show_tokens)) = tricks.get(*active.collection()) else {
            commands.entity(entity).try_despawn();
            continue;
        };
        let Ok((member, show)) = shows.get(run.0) else {
            commands.entity(entity).try_despawn();
            continue;
        };
        let token_matches =
            tokens
                .iter_many(show_tokens.iter())
                .flatten()
                .any(|(token, claim, owner)| {
                    token.slot == trick.slot
                        && claim.execution_id == event.execution_id
                        && owner.parent() == event.performer
                });
        if !token_matches {
            commands.entity(entity).try_despawn();
            continue;
        }
        let failure = AssetId::from_key("trickfailure");
        let success = AssetId::from_key("tricksuccess");
        let critical = AssetId::from_key("trickcritical");
        if let (false, Some(rewards), Some(definition)) = (
            trick.scored,
            rewards.get(&reward_source.0),
            definitions.find_trick(trick.definition),
        ) {
            let popularity = scaled_popularity(
                popularity.get(member.root).ok(),
                definition,
                &rewards.popularity,
            );
            let score = if event.policy.name == failure {
                let failures = show.failed_tricks.saturating_add(1) as f32;
                let counted = (f32::from(rewards.failures_to_ignore) - failures).max(0.0);
                Some(
                    (rewards.popularity.maximum - popularity)
                        * counted
                        * rewards.failure_multiplier,
                )
            } else if event.policy.name == success {
                Some(popularity * rewards.success_multiplier)
            } else if event.policy.name == critical {
                Some(popularity * rewards.critical_multiplier)
            } else {
                None
            };
            if let Some(score) = score {
                scores.write(ScheduledShowTrickScored {
                    show: run.0,
                    performer: event.performer,
                    slot: trick.slot,
                    trick: trick.definition,
                    score,
                    failed: event.policy.name == failure,
                });
            }
        }
        let release = clock
            .tick
            .saturating_add(event.policy.time_in_seconds.map_or(0, ticks));
        commands.entity(entity).insert((
            ShowEventForRun(run.0),
            ShowEventWindow {
                release,
                trick: trick.definition,
                expires: event
                    .policy
                    .timeout_seconds
                    .map(|seconds| release.saturating_add(ticks(seconds))),
            },
        ));
    }
}

pub(super) fn update_authored_show_event_windows(
    mut commands: Commands,
    clock: Single<&ZooClock>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    events: Query<(
        Entity,
        &WorldMember,
        &AuthoredAnimalShowEvent,
        &ShowEventWindow,
        &ShowEventForRun,
        Has<ReleasedShowEvent>,
    )>,
    performers: Query<(&WorldMember, Option<&BehaviorTaskExecutionState>), With<Animal>>,
    interruptions: Query<(), Or<(With<PendingBehaviorTaskFailure>, With<PendingBehaviorSet>)>>,
    shows: Query<&RunningShow>,
) {
    if definitions.get(&assets).is_none() {
        return;
    }
    for (entity, world, event, window, run, released) in &events {
        let live = performers
            .get(event.performer)
            .is_ok_and(|(performer_world, task)| {
                performer_world.root == world.root
                    && (!event.policy.expire_with_task
                        || (!interruptions.contains(event.performer)
                            && task.is_some_and(|task| {
                                task.execution_id == event.execution_id
                                    && task.phase != BehaviorTaskExecutionPhase::Failure
                            })))
            });
        let now_released = clock.tick >= window.release;
        if !live
            || window.expires.is_some_and(|expiry| clock.tick > expiry)
            || now_released
                && shows
                    .get(run.0)
                    .is_ok_and(|show| show.phase == ShowPhase::BetweenShows)
        {
            commands.entity(entity).try_despawn();
        } else if now_released && !released {
            commands.entity(entity).insert(ReleasedShowEvent(run.0));
        }
    }
}

pub(super) fn discard_delivered_event_when_its_window_retires(
    retired: On<Remove<ReleasedShowEvent>>,
    events: Query<&ReleasedShowEvent>,
    shows: Query<&RunningShowOnStage>,
    audiences: Query<&StageShowAudience>,
    mut history: Query<&mut GuestShowExperience>,
) {
    let Some(audience) = events
        .get(retired.entity)
        .ok()
        .and_then(|event| shows.get(event.0).ok())
        .and_then(|stage| audiences.get(stage.0).ok())
    else {
        return;
    };
    for guest in audience.iter() {
        if let Ok(mut experience) = history.get_mut(guest) {
            if experience.delivered_events.contains(&retired.entity) {
                experience.delivered_events.remove(&retired.entity);
            }
        }
    }
}

pub(super) fn dispatch_released_animal_show_events(
    mut commands: Commands,
    clock: Single<&ZooClock>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    behavior_documents: Res<Assets<BehaviorDocumentAsset>>,
    behavior_index: Res<LoadedBehaviorDocumentCollection>,
    events: Query<(&AuthoredAnimalShowEvent, &ShowEventWindow), With<ReleasedShowEvent>>,
    performers: Query<&SpeciesHandle, With<Animal>>,
    shows: Query<(&RunningShowOnStage, &RunningShow, &ReleasedShowEventsForRun)>,
    stages: Query<&StageRunningShow>,
    newly_released: Query<(Entity, &ShowEventWindow, &ReleasedShowEvent), Added<ReleasedShowEvent>>,
    retry_watches: Query<
        (Entity, &GuestWatchingStage),
        Or<(Changed<GuestWatchingStage>, With<PendingShowReaction>)>,
    >,
    audiences: Query<&StageShowAudience>,
    mut guests: Query<(
        &GuestWatchingStage,
        &RunningBehaviorInstruction,
        &RunBehaviorWatchShow,
        &GuestArchetype,
        &GuestFavouriteAnimal,
        &mut GuestShowExperience,
        Option<&mut GuestRng>,
        Option<&mut GuestSurveyExperience>,
    )>,
    mut reactions: MessageWriter<ShowAudienceReaction>,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let behavior = behavior_index.create_declaration_index_view(&behavior_documents);
    let mut ready = std::collections::BTreeMap::<Entity, bevy::ecs::entity::EntityHashSet>::new();
    for (_, _, run) in &newly_released {
        if let Ok((stage, _, _)) = shows.get(run.0) {
            if let Ok(audience) = audiences.get(stage.0) {
                ready.entry(run.0).or_default().extend(audience.iter());
            }
        }
    }
    for (guest, watch) in &retry_watches {
        if let Ok(run) = stages.get(watch.stage) {
            ready.entry(*run.collection()).or_default().insert(guest);
        } else {
            commands.entity(guest).try_remove::<PendingShowReaction>();
        }
    }
    for (show_entity, eligible) in ready {
        let Ok((stage, show, released)) = shows.get(show_entity) else {
            for guest in eligible {
                commands.entity(guest).try_remove::<PendingShowReaction>();
            }
            continue;
        };
        let stage = stage.0;
        let quality = (show.stars * 65_536.0).round() as i32;
        if let Ok(audience) = audiences.get(stage) {
            for guest in audience.iter().filter(|guest| eligible.contains(guest)) {
                if show.phase == ShowPhase::BetweenShows {
                    commands.entity(guest).try_remove::<PendingShowReaction>();
                    continue;
                }
                let Ok((
                    watch,
                    instruction,
                    action,
                    archetype,
                    favorite,
                    mut history,
                    random,
                    survey,
                )) = guests.get_mut(guest)
                else {
                    continue;
                };
                if watch.instruction != Some(instruction.id()) {
                    commands.entity(guest).try_remove::<PendingShowReaction>();
                    continue;
                }
                if history.show != Some(show_entity) {
                    history.show = Some(show_entity);
                    history.reactions.clear();
                    history.delivered_events.clear();
                    history.saw_favourite = false;
                }
                let policies = &action.0.events;
                let default = policies
                    .iter()
                    .find(|rule| rule.event == AssetId::from_key("defaultevent"));
                let mut retry_count = 0;
                let candidate = released
                    .iter()
                    .filter(|entity| !history.delivered_events.contains(entity))
                    .filter_map(|entity| {
                        let (event, window) = events.get(entity).ok()?;
                        let rule = policies
                            .iter()
                            .find(|rule| rule.event == event.policy.name)
                            .or(default)?;
                        retry_count += usize::from(window.expires.is_some());
                        Some((entity, event, window, rule))
                    })
                    .max_by_key(|(entity, event, _, rule)| {
                        (
                            event.policy.interrupt,
                            (i64::from(event.policy.priority) << 16)
                                + i64::from(rule.priority_q16.unwrap_or_default()),
                            std::cmp::Reverse(*entity),
                        )
                    });
                let Some((entity, event, window, policy)) = candidate else {
                    commands.entity(guest).try_remove::<PendingShowReaction>();
                    continue;
                };
                if window.expires.is_some() {
                    history.delivered_events.insert(entity);
                    retry_count -= 1;
                }
                if retry_count > 0 {
                    commands.entity(guest).try_insert(PendingShowReaction);
                } else {
                    commands.entity(guest).try_remove::<PendingShowReaction>();
                }
                let popularity = definitions
                    .find_trick(window.trick)
                    .map_or(0, |trick| trick.popularity_q16);
                let species = performers
                    .get(event.performer)
                    .ok()
                    .map(|species| species.species);
                let favourite = species.is_some_and(|species| favorite.0 == Some(species));
                history.saw_favourite |= favourite;
                let previous = history.reactions.entry(event.policy.name).or_default();
                let score =
                    score_show_event(policy, popularity, quality, favourite, *previous, random);
                *previous = previous.saturating_add(1);
                let reaction = policy
                    .reactions
                    .iter()
                    .filter(|reaction| score >= reaction.minimum_score_q16)
                    .max_by_key(|reaction| reaction.minimum_score_q16);
                let Some(reaction) = reaction else { continue };
                let types = definitions
                    .find_guest(archetype.0)
                    .map_or(&[][..], |guest| {
                        guest.behavior_subject_type_identifiers.as_slice()
                    });
                let Some((document, declaration)) = behavior.as_ref().and_then(|view| {
                    view.find_behavior_set_location(reaction.behavior_set, types.iter().copied())
                }) else {
                    continue;
                };
                if let Some(mut survey) = survey {
                    survey.observe_entertainment(score as f32 / 65_536.0);
                }
                reactions.write(ShowAudienceReaction {
                    guest,
                    show: show_entity,
                    stage,
                    event: event.policy.name,
                    score_q16: score,
                });
                instruction.enter_set(
                    guest,
                    SelectedBehaviorSet {
                        program: reaction.behavior_set,
                        document,
                        declaration,
                        tick: clock.tick,
                        repeat: None,
                        target: None,
                    },
                    &mut commands,
                );
            }
        }
    }
    for (entity, window, _) in &newly_released {
        if window.expires.is_none() {
            commands.entity(entity).try_despawn();
        }
    }
}

fn score_show_event(
    policy: &ShowEventScoring,
    popularity: i32,
    quality: i32,
    favorite: bool,
    previous: u32,
    random: Option<Mut<GuestRng>>,
) -> i32 {
    let product = |value: i32, multiplier: Option<i32>| {
        i64::from(value) * i64::from(multiplier.unwrap_or_default()) / 65_536
    };
    let ignore = (policy.ignore_previous_q16.unwrap_or_default() >> 16).max(0) as u32;
    let counted = previous.saturating_sub(ignore);
    let fuzz = random
        .zip(policy.fuzzy_factor_q16.filter(|&range| range != 0))
        .map_or(0, |(mut random, range)| {
            let sample = i64::from(random.0.next_u32() >> 16);
            (sample * 2 - 65_535) * i64::from(range) / 65_535
        });
    let score = i64::from(policy.base_score_q16.unwrap_or_default())
        + fuzz
        + i64::from(
            policy
                .favorite_animal_bonus_q16
                .filter(|_| favorite)
                .unwrap_or_default(),
        )
        + product(popularity, policy.trick_popularity_multiplier_q16)
        + product(quality, policy.show_quality_multiplier_q16)
        + i64::from(counted) * i64::from(policy.per_previous_q16.unwrap_or_default());
    score.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}
