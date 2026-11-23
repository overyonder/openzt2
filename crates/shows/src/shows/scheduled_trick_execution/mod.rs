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
        behavior_random_stream_state::BehaviorRandomStream, behavior_token_types::BehaviorSelfToken,
    },
    animal_health::types::Dead,
    animal_lifecycle::types::Animal,
    progression::fame_types::Fame,
    shows::{
        performer_training::PerformerTraining,
        running_show_types::{BeginScheduledShowTrick, RunningShowTrick, WorldTrickPopularity},
    },
};

use super::{
    authored_show_rewards::{AuthoredShowRewards, AuthoredShowRewardsSource},
    show_scoring::{decay_popularity, zoo_fame},
    show_trick_props::ShowPropPlacement,
};

#[derive(Component)]
#[relationship(relationship_target = ShowTrickTokens)]
pub(super) struct TrickTokenForShow(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = TrickTokenForShow, linked_spawn)]
pub(super) struct ShowTrickTokens(Vec<Entity>);

#[derive(Component)]
pub(super) struct ScheduledTrickToken {
    pub(super) slot: Entity,
}

pub(super) fn start_scheduled_tricks_with_authored_outcome_tokens(
    mut commands: Commands,
    mut requests: MessageReader<BeginScheduledShowTrick>,
    clock: Single<&ZooClock>,
    rewards_source: Res<AuthoredShowRewardsSource>,
    rewards: Res<Assets<AuthoredShowRewards>>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut performers: Query<
        (
            &WorldMember,
            Option<&PerformerTraining>,
            &mut BehaviorRandomStream,
        ),
        (With<Animal>, Without<Dead>),
    >,
    mut props: ShowPropPlacement,
    mut popularity: Query<(&mut WorldTrickPopularity, Option<&Fame>), With<WorldRoot>>,
) {
    let (Some(definitions), Some(rewards)) =
        (definitions.get(&assets), rewards.get(&rewards_source.0))
    else {
        requests.clear();
        return;
    };
    for request in requests.read() {
        let (Some(trick), Some(tokens), Ok((world, training, mut random))) = (
            definitions.find_trick(request.trick),
            definitions.find_trick_outcome_tokens(request.trick),
            performers.get_mut(request.performer),
        ) else {
            continue;
        };
        let Some(level) = performance_level(trick, training) else {
            continue;
        };
        let roll = random.range_u32(100).unwrap_or_default();
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
            continue;
        };
        if name == AssetId::default() {
            continue;
        }
        let Some(target) = props.spawn(
            definitions,
            trick,
            request.stage,
            request.performer,
            request.show,
            &mut commands,
        ) else {
            continue;
        };
        if let Ok((mut popularity, fame)) = popularity.get_mut(world.root) {
            decay_popularity(&mut popularity, trick, zoo_fame(fame), &rewards.popularity);
        }
        let token_lifetime = (f64::from(rewards.trick_token_timeout_seconds)
            * f64::from(definitions.timing().fixed_hz))
        .ceil() as u64;
        commands.spawn((
            ChildOf(request.performer),
            TrickTokenForShow(request.show),
            ScheduledTrickToken { slot: request.slot },
            BehaviorSelfToken {
                name,
                available_at: clock.tick,
                expires_at: clock
                    .tick
                    .saturating_add(token_lifetime.min(u64::from(request.duration_ticks))),
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

pub(super) fn release_tokens_from_completed_trick_slots(
    mut commands: Commands,
    runs: Query<(Option<&RunningShowTrick>, &ShowTrickTokens)>,
    tokens: Query<&ScheduledTrickToken>,
) {
    for (current, associated_tokens) in &runs {
        for token in associated_tokens.iter() {
            if tokens
                .get(token)
                .is_ok_and(|token| current.is_none_or(|trick| trick.slot != token.slot))
            {
                commands.entity(token).try_despawn();
            }
        }
    }
}
