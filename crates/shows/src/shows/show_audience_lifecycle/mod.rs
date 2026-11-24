use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;

use super::{
    authored_show_rewards::{AuthoredShowRewards, AuthoredShowRewardsSource},
    show_audience_types::{GuestShowExperience, GuestWatchingStage, StageShowAudience},
};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    animal_behavior::{
        behavior_set_start_request_types::PendingBehaviorSet,
        behavior_token_types::{BehaviorSelfToken, ClaimedBehaviorSelfToken},
    },
    behavior_task_execution_types::{
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
        running_actions::{RunBehaviorWatchShow, RunningBehaviorInstruction},
    },
    economy::{
        authored_transaction_amount::authored_currency_to_money,
        experience_donation::ExperienceDonation, facility_economy_types::Wallet,
        service_types::ServiceReservation,
    },
    guests::guest_simulation_types::{Guest, GuestPhase, GuestRng},
    locomotion::locomotion_types::{Destination, SpatialGrid, SpatialGridMembership},
    shows::{
        running_show_types::{
            GuestShowNeed, RunningShow, RunningShowFromRow, RunningShowOnStage, ShowPhase,
            ShowRunFinished, ShowViewingLocationOf,
        },
        show_schedule_types::ScheduledShowRating,
        show_stage_types::{ShowStage, ShowStageOpenState},
    },
};

pub(super) fn invite_guests_to_starting_shows(
    runs: Query<(&RunningShow, &RunningShowOnStage, &RunningShowFromRow), Added<RunningShow>>,
    stages: Query<(&ShowStage, &WorldMember, &GlobalTransform)>,
    ratings: Query<&ScheduledShowRating>,
    guests: Query<
        (
            Entity,
            &GuestPhase,
            &WorldMember,
            &GlobalTransform,
            Option<&Children>,
            &GuestShowNeed,
        ),
        (With<Guest>, Without<GuestWatchingStage>),
    >,
    tokens: Query<&BehaviorSelfToken>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    clock: Single<&ZooClock>,
    grids: Query<&SpatialGrid>,
    spatial_membership: SpatialGridMembership,
    mut commands: Commands,
) {
    if runs.is_empty() {
        return;
    }
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let Some(timing) = definitions.show_scheduling() else {
        return;
    };
    let available_ns = timing.pre_show_ns.min(timing.maximum_admitting_ns);
    let duration = (u128::from(available_ns) * u128::from(definitions.timing().fixed_hz))
        .div_ceil(1_000_000_000);
    let expires_at = clock
        .tick
        .saturating_add(u64::try_from(duration).unwrap_or(u64::MAX));
    let radius = timing.maximum_travel_cm_per_second as f64 * available_ns as f64 * 1.0e-11;
    let radius_squared = (radius * radius) as f32;
    let invite = AssetId::from_key("t_watchshow");
    let mut invited = bevy::ecs::entity::EntityHashSet::new();
    let mut starting = runs.iter().collect::<Vec<_>>();
    starting.sort_unstable_by_key(|(_, stage, _)| stage.0);
    for (run, stage, row) in starting {
        if run.phase != ShowPhase::Preshow {
            continue;
        }
        let Ok((definition, world, pose)) = stages.get(stage.0) else {
            continue;
        };
        let Some(definition) = definitions.find_show_stage(definition.show_stage_definition) else {
            continue;
        };
        let stars = ratings
            .get(row.0)
            .ok()
            .and_then(|rating| rating.0)
            .unwrap_or_default();
        let admission_stars = ((2.0 * stars).ceil() * 0.5).clamp(0.5, 5.0);
        let capacity = definitions
            .show_scoring()
            .and_then(|policy| {
                policy
                    .size_bands
                    .iter()
                    .filter(|band| band.minimum_show_score <= admission_stars)
                    .max_by(|left, right| {
                        left.minimum_show_score.total_cmp(&right.minimum_show_score)
                    })
                    .map(|band| band.guest_count[1])
            })
            .unwrap_or(definition.audience_capacity);
        let Ok(grid) = grids.get(world.root) else {
            continue;
        };
        let Some(centre) = grid
            .cell_of(pose.translation().xz())
            .and_then(|cell| grid.cell_xy(cell))
        else {
            continue;
        };
        let cell_radius = (radius / f64::from(grid.cell_size_m)).ceil() as u32;
        let min = UVec2::new(
            centre.x.saturating_sub(cell_radius),
            centre.y.saturating_sub(cell_radius),
        );
        let max = UVec2::new(
            centre.x.saturating_add(cell_radius).min(grid.width - 1),
            centre.y.saturating_add(cell_radius).min(grid.height - 1),
        );
        let nearby_members = &spatial_membership;
        let nearby_entities = (min.y..=max.y).flat_map(|y| {
            (min.x..=max.x)
                .flat_map(move |x| nearby_members.members(world.root, y * grid.width + x))
        });
        let mut nearby = guests
            .iter_many(nearby_entities)
            .filter_map(Result::ok)
            .filter_map(|(guest, phase, member, position, children, need)| {
                if *phase != GuestPhase::Visiting
                    || world.root != member.root
                    || invited.contains(&guest)
                    || children.is_some_and(|children| {
                        tokens
                            .iter_many(children.iter())
                            .flatten()
                            .any(|token| token.name == invite && clock.tick < token.expires_at)
                    })
                {
                    return None;
                }
                let distance = pose
                    .translation()
                    .xz()
                    .distance_squared(position.translation().xz());
                (distance <= radius_squared).then_some((guest, need.value))
            })
            .collect::<Vec<_>>();
        nearby.sort_unstable_by(|(left, left_need), (right, right_need)| {
            right_need
                .total_cmp(left_need)
                .then_with(|| left.cmp(right))
        });
        for (guest, _) in nearby.into_iter().take(usize::from(capacity)) {
            invited.insert(guest);
            commands.spawn((
                ChildOf(guest),
                BehaviorSelfToken {
                    name: invite,
                    available_at: clock.tick,
                    expires_at,
                    payload: Some(stage.0),
                    retain_on_failure: true,
                },
            ));
        }
    }
}

pub(super) fn bind_ready_show_watch_actions(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
    mut watching: Query<
        (
            Entity,
            &GuestPhase,
            &WorldMember,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
        ),
        (
            With<Guest>,
            With<RunBehaviorWatchShow>,
            Without<GuestWatchingStage>,
        ),
    >,
    blocked: Query<
        (),
        Or<(
            With<PendingBehaviorTaskFailure>,
            With<PendingBehaviorSet>,
            With<Destination>,
            With<ServiceReservation>,
        )>,
    >,
    stages: Query<
        (
            &WorldMember,
            Option<&ShowStageOpenState>,
            &openzt2_zoo_state::shows::running_show_types::StageRunningShow,
        ),
        With<ShowStage>,
    >,
    runs: Query<&RunningShow>,
    tokens: Query<(&ChildOf, &BehaviorSelfToken, &ClaimedBehaviorSelfToken)>,
    targets: Query<(&WorldMember, Option<&Children>), Without<ShowStage>>,
    viewing_locations: Query<&ShowViewingLocationOf>,
) {
    for (guest, phase, world, task, instruction) in
        behavior_action_actors(&mut watching, activation.0)
    {
        if activation.0.is_none() && !instruction.can_update_after_activation(clock.tick) {
            continue;
        }
        if *phase != GuestPhase::Visiting
            || task.next_action_tick > clock.tick
            || blocked.contains(guest)
            || !instruction.is_current(task)
        {
            continue;
        }
        let (Some(target), Some(token)) = (task.target, task.object) else {
            continue;
        };
        let Ok((owner, invite, claim)) = tokens.get(token) else {
            continue;
        };
        if owner.parent() != guest
            || invite.name != AssetId::from_key("t_watchshow")
            || claim.execution_id != task.execution_id
        {
            continue;
        }
        let Some(stage) = invite.payload else {
            continue;
        };
        let Ok((target_world, children)) = targets.get(target) else {
            continue;
        };
        if target_world.root != world.root
            || !stages.get(stage).is_ok_and(|(member, open, run)| {
                member.root == world.root
                    && open != Some(&ShowStageOpenState::Closed)
                    && runs
                        .get(*run.collection())
                        .is_ok_and(|run| run.phase != ShowPhase::BetweenShows)
            })
        {
            openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation(guest, &mut commands);
            continue;
        }
        let Some(location) = children
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .find(|&location| {
                viewing_locations
                    .get(location)
                    .is_ok_and(|owner| owner.0 == stage)
            })
        else {
            openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation(guest, &mut commands);
            continue;
        };
        commands.entity(guest).insert(GuestWatchingStage {
            stage,
            location: Some(location),
            instruction: Some(instruction.id()),
        });
    }
}

pub(super) fn discard_experience_when_guest_stops_watching(
    removed: On<Remove<GuestWatchingStage>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .try_remove::<GuestShowExperience>();
}

pub(super) fn retire_interrupted_show_watch_actions(
    watches: Query<
        (
            Entity,
            &GuestWatchingStage,
            &GuestPhase,
            &RunningBehaviorInstruction,
        ),
        (With<RunBehaviorWatchShow>, With<Guest>),
    >,
    blocked: Query<
        (),
        Or<(
            With<PendingBehaviorTaskFailure>,
            With<PendingBehaviorSet>,
            With<Destination>,
            With<ServiceReservation>,
        )>,
    >,
    viewing_locations: Query<&ShowViewingLocationOf>,
    mut commands: Commands,
) {
    for (guest, watch, phase, instruction) in &watches {
        if *phase != GuestPhase::Visiting
            || watch.instruction != Some(instruction.id())
            || blocked.contains(guest)
            || watch
                .location
                .is_none_or(|location| !viewing_locations.contains(location))
        {
            commands.entity(guest).try_remove::<GuestWatchingStage>();
        }
    }
}

pub(super) fn clear_completed_show_watch_action(
    removed: On<
        Remove<(
            RunBehaviorWatchShow,
            Guest,
            GuestPhase,
            RunningBehaviorInstruction,
        )>,
    >,
    linked: Query<(), With<GuestWatchingStage>>,
    mut commands: Commands,
) {
    if linked.contains(removed.entity) {
        commands
            .entity(removed.entity)
            .try_remove::<GuestWatchingStage>();
    }
}

pub(super) fn complete_watch_actions_after_show(
    mut finished: MessageReader<ShowRunFinished>,
    mut cancelled: MessageReader<super::running_show_schedule::ShowRunCancelled>,
    audiences: Query<&StageShowAudience>,
    mut guests: Query<
        (
            &GuestWatchingStage,
            &RunningBehaviorInstruction,
            &GuestShowExperience,
            &Wallet,
            &mut GuestRng,
            Option<&mut GuestShowNeed>,
        ),
        With<RunBehaviorWatchShow>,
    >,
    clock: Single<&ZooClock>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    rewards_source: Res<AuthoredShowRewardsSource>,
    rewards: Res<Assets<AuthoredShowRewards>>,
    runs: Query<&RunningShow>,
    mut commands: Commands,
) {
    let reward_policy = rewards.get(&rewards_source.0);
    let timeout = definitions.get(&definition_assets).and_then(|definitions| {
        let timing = definitions.show_scheduling()?;
        let nanos = if timing.donation_token_timeout_ns == 0 {
            60_000_000_000
        } else {
            timing.donation_token_timeout_ns
        };
        let ticks =
            (u128::from(nanos) * u128::from(definitions.timing().fixed_hz)).div_ceil(1_000_000_000);
        u64::try_from(ticks).ok()
    });
    let outcomes = finished
        .read()
        .map(|show| {
            let score = runs.get(show.show).ok().map(|run| {
                (
                    u16::try_from(run.completed_tricks).unwrap_or(u16::MAX),
                    run.trick_score,
                )
            });
            (show.stage, show.show, score, score.is_some())
        })
        .chain(
            cancelled
                .read()
                .map(|show| (show.stage, show.show, None, true)),
        );
    for (stage, show, score, can_reward) in outcomes {
        let Ok(audience) = audiences.get(stage) else {
            continue;
        };
        for guest in audience.iter() {
            let Ok((watch, instruction, experience, wallet, mut random, need)) =
                guests.get_mut(guest)
            else {
                continue;
            };
            if watch.instruction != Some(instruction.id()) {
                continue;
            }
            if let (Some(rewards), Some(timeout), true) = (reward_policy, timeout, can_reward) {
                let favourite = experience.show == Some(show) && experience.saw_favourite;
                let (dollars, need_update) =
                    rewards.audience_feedback(score, favourite, || random.0.next_u32());
                if let Some(mut need) = need {
                    need.adjust(-need_update);
                }
                if let Some(amount) = authored_currency_to_money(dollars)
                    .filter(|amount| amount.is_positive() && *amount <= wallet.0)
                {
                    commands.spawn((
                        ChildOf(guest),
                        BehaviorSelfToken {
                            name: AssetId::from_key("t_donationshow"),
                            available_at: clock.tick,
                            expires_at: clock.tick.saturating_add(timeout),
                            payload: Some(stage),
                            retain_on_failure: false,
                        },
                        ExperienceDonation {
                            subject: stage,
                            amount,
                            submitted: false,
                        },
                    ));
                }
            }
            instruction.complete(guest, clock.tick, &mut commands);
        }
    }
}
