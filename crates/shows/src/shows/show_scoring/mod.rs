use bevy::{
    ecs::{entity::EntityHashMap, relationship::RelationshipTarget},
    prelude::*,
};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::world_definitions::animal_shows_and_training::{
    ShowScoringPolicy, TrickDefinition,
};
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::{
    animal_health::types::Dead,
    animal_lifecycle::types::Animal,
    progression::fame_types::Fame,
    shows::{
        running_show_types::{
            AnimalPerformingShowTrick, RunningShow, RunningShowFromRow, RunningShowTrick,
            ScheduledShowTrickScored, ShowPhase, ShowRunFinished, WorldTrickPopularity,
        },
        show_schedule_types::{
            ScheduledShowRating, ScheduledShowTrickPerformance, ScheduledTrickPerformer,
        },
    },
};

use super::{
    authored_show_rewards::{
        AuthoredShowRewards, AuthoredShowRewardsSource, TrickPopularityPolicy,
    },
    running_show_schedule::{ActiveShowTrickOf, ShowRunSlotOrder},
    scheduled_trick_execution::performance_level,
};

pub(super) fn scaled_popularity(
    current: Option<&WorldTrickPopularity>,
    trick: &TrickDefinition,
    policy: &TrickPopularityPolicy,
) -> f32 {
    let value = current
        .and_then(|world| world.0.get(&trick.id))
        .copied()
        .unwrap_or(policy.maximum);
    value * (trick.popularity_q16 as f32 / 65_536.0)
}

pub(super) fn decay_popularity(
    world: &mut WorldTrickPopularity,
    trick: &TrickDefinition,
    fame: f32,
    policy: &TrickPopularityPolicy,
) {
    let popularity = world.0.entry(trick.id).or_insert(policy.maximum);
    let decay = fame.max(0.5) / trick.difficulty.max(0.5) * policy.decrement_multiplier;
    *popularity = (*popularity - decay).max(policy.minimum);
}

pub(super) fn grow_trick_popularity(
    source: Res<AuthoredShowRewardsSource>,
    rewards: Res<Assets<AuthoredShowRewards>>,
    time: Res<Time<Fixed>>,
    mut worlds: Query<&mut WorldTrickPopularity, With<WorldRoot>>,
) {
    let Some(rewards) = rewards.get(&source.0) else {
        return;
    };
    let growth = rewards.popularity.growth_per_second * time.delta_secs();
    if growth == 0.0 {
        return;
    }
    for mut world in &mut worlds {
        for value in world.0.values_mut() {
            *value = (*value + growth).min(rewards.popularity.maximum);
        }
    }
}

#[derive(Component)]
pub struct ShowScoreSamples {
    pub predicted: EntityHashMap<f32>,
    pub actual: EntityHashMap<f32>,
}

pub(super) fn predict_performance_scores(
    runs: Query<(Entity, &WorldMember, &ShowRunSlotOrder, &RunningShow), Without<ShowScoreSamples>>,
    slots: Query<(&ScheduledShowTrickPerformance, &ScheduledTrickPerformer)>,
    performers: Query<
        Option<&openzt2_zoo_state::shows::performer_training::PerformerTraining>,
        (With<Animal>, Without<Dead>),
    >,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    popularity: Query<&WorldTrickPopularity, With<WorldRoot>>,
    source: Res<AuthoredShowRewardsSource>,
    rewards: Res<Assets<AuthoredShowRewards>>,
    mut commands: Commands,
) {
    let (Some(definitions), Some(rewards)) = (definitions.get(&assets), rewards.get(&source.0))
    else {
        return;
    };
    for (entity, member, order, run) in &runs {
        if !matches!(run.phase, ShowPhase::Preshow | ShowPhase::Conducting) {
            continue;
        }
        let predicted = order
            .slots()
            .filter_map(|slot| {
                let (performance, performer) = slots.get(slot).ok()?;
                let trick = definitions.find_trick(performance.trick)?;
                performance_level(trick, performers.get(performer.0).ok()?)?;
                Some((
                    slot,
                    scaled_popularity(popularity.get(member.root).ok(), trick, &rewards.popularity)
                        * rewards.success_multiplier,
                ))
            })
            .collect();
        commands.entity(entity).insert(ShowScoreSamples {
            predicted,
            actual: EntityHashMap::default(),
        });
    }
}

pub(super) fn apply_authored_trick_outcome_scores(
    mut results: MessageReader<ScheduledShowTrickScored>,
    performers: Query<&AnimalPerformingShowTrick>,
    mut actions: Query<(&ActiveShowTrickOf, &mut RunningShowTrick)>,
    mut shows: Query<(&mut RunningShow, &mut ShowScoreSamples)>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    source: Res<AuthoredShowRewardsSource>,
    rewards: Res<Assets<AuthoredShowRewards>>,
) {
    let (Some(policy), Some(rewards)) = (
        definitions
            .get(&assets)
            .and_then(|definitions| definitions.show_scoring()),
        rewards.get(&source.0),
    ) else {
        results.clear();
        return;
    };
    for result in results.read() {
        let Ok(action) = performers.get(result.performer) else {
            continue;
        };
        let Ok((run, mut trick)) = actions.get_mut(*action.collection()) else {
            continue;
        };
        let Ok((mut show, mut samples)) = shows.get_mut(result.show) else {
            continue;
        };
        if show.phase != ShowPhase::Conducting
            || trick.definition != result.trick
            || trick.slot != result.slot
            || trick.scored
            || run.0 != result.show
        {
            continue;
        }
        trick.scored = true;
        samples.actual.insert(result.slot, result.score);
        show.completed_tricks = show.completed_tricks.saturating_add(1);
        show.failed_tricks = show.failed_tricks.saturating_add(u32::from(result.failed));
        (show.trick_score, show.stars) = samples.rating(policy, rewards.score_fraction);
    }
}

impl ShowScoreSamples {
    fn rating(&self, policy: &ShowScoringPolicy, fraction: f32) -> (f32, f32) {
        let mut scores: Vec<_> =
            self.actual
                .values()
                .copied()
                .chain(self.predicted.iter().filter_map(|(slot, &score)| {
                    (!self.actual.contains_key(slot)).then_some(score)
                }))
                .collect();
        scores.sort_unstable_by(f32::total_cmp);
        let count = ((scores.len() as f32 * fraction) as usize).min(scores.len());
        if count == 0 || self.actual.is_empty() {
            return (0.0, 0.0);
        }
        let negatives = scores.partition_point(|score| *score < 0.0).min(count);
        let sum: f32 = scores[..negatives]
            .iter()
            .chain(scores.iter().rev().take(count - negatives))
            .copied()
            .sum();
        let average = sum / count as f32;
        let stars = policy
            .score_bands
            .iter()
            .filter(|band| {
                usize::from(band.minimum_tricks) <= self.actual.len()
                    && band.minimum_score <= average
            })
            .max_by(|left, right| {
                left.minimum_tricks
                    .cmp(&right.minimum_tricks)
                    .then_with(|| left.minimum_score.total_cmp(&right.minimum_score))
            })
            .map_or(0.0, |band| band.show_score);
        (average, stars)
    }
}

pub(super) fn smooth_finished_show_scores(
    mut finished: MessageReader<ShowRunFinished>,
    runs: Query<(&RunningShow, &RunningShowFromRow)>,
    mut rows: Query<&mut ScheduledShowRating>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
) {
    let Some(policy) = definitions
        .get(&assets)
        .and_then(|definitions| definitions.show_scoring())
    else {
        finished.clear();
        return;
    };
    for result in finished.read() {
        let Ok((run, row)) = runs.get(result.show) else {
            continue;
        };
        if run.stars != 0.0 {
            if let Ok(mut current) = rows.get_mut(row.0) {
                let factor = policy.score_update_factor;
                current.0 = Some(current.0.map_or(run.stars, |prior| {
                    factor * run.stars + (1.0 - factor) * prior
                }));
            }
        }
    }
}

pub(super) fn zoo_fame(fame: Option<&Fame>) -> f32 {
    fame.map_or(0.0, |fame| f32::from(fame.half_stars) * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::world_definitions::animal_shows_and_training::ShowScoreBand;

    #[test]
    fn rating_replaces_predictions_then_keeps_low_failures_and_high_successes() {
        let mut world = World::new();
        let slots: [Entity; 4] = std::array::from_fn(|_| world.spawn_empty().id());
        let samples = ShowScoreSamples {
            predicted: slots.into_iter().zip([100.0, 90.0, 60.0, 80.0]).collect(),
            actual: [(slots[0], 40.0), (slots[1], -20.0)].into_iter().collect(),
        };
        let policy = ShowScoringPolicy {
            minimum_score: 0.0,
            percent_tricks_to_score: 0.5,
            score_update_factor: 0.5,
            score_bands: vec![
                ShowScoreBand {
                    minimum_tricks: 1,
                    minimum_score: 20.0,
                    show_score: 1.5,
                },
                ShowScoreBand {
                    minimum_tricks: 2,
                    minimum_score: 30.0,
                    show_score: 2.0,
                },
                ShowScoreBand {
                    minimum_tricks: 3,
                    minimum_score: 30.0,
                    show_score: 3.0,
                },
            ],
            size_bands: Vec::new(),
        };
        assert_eq!(samples.rating(&policy, 0.5), (30.0, 2.0));
    }
}
