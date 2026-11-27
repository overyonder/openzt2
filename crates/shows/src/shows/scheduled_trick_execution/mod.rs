use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::{
    world_definitions::animal_shows_and_training::{TrickDefinition, TrickLevel},
    AssetId,
};
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::{WorldMember, WorldRoot},
};
use openzt2_zoo_state::{
    animal_behavior::{
        behavior_random_stream_state::BehaviorRandomStream,
        behavior_token_types::{BehaviorSelfToken, BehaviorTrickDestination},
    },
    animal_health::types::Dead,
    animal_lifecycle::types::Animal,
    progression::fame_types::Fame,
    shows::{
        performer_training::PerformerTraining,
        running_show_types::{
            AnimalPerformingShowTrick, RunningShowOnStage, RunningShowTrick,
            ShowTrickBeingPerformedBy, WorldTrickPopularity,
        },
    },
};

use super::{
    authored_show_rewards::{AuthoredShowRewards, AuthoredShowRewardsSource},
    running_show_schedule::ActiveShowTrickOf,
    show_scoring::{decay_popularity, zoo_fame},
    show_timing_operations::{fixed_ticks_for_seconds, RunningShowPhaseTimer},
    show_trick_props::ShowPropPlacement,
};

#[derive(Component)]
#[relationship(relationship_target = ShowTrickTokens)]
pub struct TrickTokenForShow(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = TrickTokenForShow, linked_spawn)]
pub struct ShowTrickTokens(Vec<Entity>);

#[derive(Component)]
pub struct ScheduledTrickToken {
    pub slot: Entity,
}

#[derive(Component)]
pub struct PendingShowTrickToken(pub u32);

pub(super) fn start_scheduled_tricks_with_authored_outcome_tokens(
    mut commands: Commands,
    actions: Query<(
        Entity,
        &ActiveShowTrickOf,
        &ShowTrickBeingPerformedBy,
        &RunningShowTrick,
        &PendingShowTrickToken,
    )>,
    shows: Query<(&RunningShowOnStage, &RunningShowPhaseTimer)>,
    clocks: Query<&ZooClock, With<WorldRoot>>,
    time: Res<Time<Fixed>>,
    rewards_source: Res<AuthoredShowRewardsSource>,
    rewards: Res<Assets<AuthoredShowRewards>>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut performers: Query<
        (
            &WorldMember,
            Option<&PerformerTraining>,
            &mut BehaviorRandomStream,
            &AnimalPerformingShowTrick,
        ),
        (With<Animal>, Without<Dead>),
    >,
    mut props: ShowPropPlacement,
    mut popularity: Query<(&mut WorldTrickPopularity, Option<&Fame>), With<WorldRoot>>,
) {
    let (Some(definitions), Some(rewards)) =
        (definitions.get(&assets), rewards.get(&rewards_source.0))
    else {
        return;
    };
    for (action, run, performer, performance, pending) in &actions {
        let (
            Some(trick),
            Some(tokens),
            Ok((world, training, mut random, active)),
            Ok((stage, phase)),
        ) = (
            definitions.find_trick(performance.definition),
            definitions.find_trick_outcome_tokens(performance.definition),
            performers.get_mut(performer.0),
            shows.get(run.0),
        )
        else {
            continue;
        };
        if *active.collection() != action {
            continue;
        }
        let Ok(clock) = clocks.get(world.root) else {
            continue;
        };
        let Some(level) = performance_level(trick, training) else {
            commands.entity(action).remove::<PendingShowTrickToken>();
            continue;
        };
        let mut next_random = *random;
        let roll = next_random.range_u32(100).unwrap_or_default();
        let name = if roll < u32::from(level.failure_percent) {
            tokens.failure
        } else if roll < u32::from(level.failure_percent) + u32::from(level.success_percent) {
            tokens.success
        } else if roll
            < u32::from(level.failure_percent)
                + u32::from(level.success_percent)
                + u32::from(level.critical_percent)
        {
            tokens.critical
        } else {
            AssetId::default()
        };
        if name == AssetId::default() {
            *random = next_random;
            commands.entity(action).remove::<PendingShowTrickToken>();
            continue;
        }
        let Some((target, position)) = props.spawn(
            definitions,
            trick,
            stage.0,
            performer.0,
            run.0,
            action,
            &mut commands,
        ) else {
            continue;
        };
        *random = next_random;
        commands.entity(action).remove::<PendingShowTrickToken>();
        if let Ok((mut popularity, fame)) = popularity.get_mut(world.root) {
            decay_popularity(&mut popularity, trick, zoo_fame(fame), &rewards.popularity);
        }
        let token_lifetime =
            fixed_ticks_for_seconds(rewards.trick_token_timeout_seconds, time.timestep());
        commands.spawn((
            ChildOf(performer.0),
            TrickTokenForShow(action),
            ScheduledTrickToken {
                slot: performance.slot,
            },
            BehaviorTrickDestination(position),
            BehaviorSelfToken {
                name,
                available_at: clock.tick,
                expires_at: clock.tick.saturating_add(token_lifetime.min(u64::from(
                    pending.0.min(phase.remaining_ticks(time.timestep())),
                ))),
                payload: Some(target),
                retain_on_failure: false,
            },
        ));
    }
}

pub(super) fn performance_level<'a>(
    trick: &'a TrickDefinition,
    training: Option<&PerformerTraining>,
) -> Option<&'a TrickLevel> {
    let proficiency = |id| training.map_or(0, |training| training.proficiency(id));
    if trick
        .prerequisite
        .is_some_and(|prerequisite| proficiency(prerequisite.trick) < prerequisite.minimum_score)
    {
        return None;
    }
    trick
        .levels
        .iter()
        .filter(|level| level.minimum_score <= proficiency(trick.id))
        .max_by_key(|level| level.minimum_score)
        .filter(|level| level.can_perform)
}
