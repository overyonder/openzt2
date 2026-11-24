//! Shared candidate ranking over borrowed canonical task records and live facts.

use crate::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex};
use crate::terrain::terrain_world_sampling::{sample_terrain_biome, terrain_chunk_at};
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::{
    behavior::eligibility::{
        BehaviorCandidateEligibilityFact, BehaviorCandidateEligibilityRequirements,
        BehaviorCandidateRole, BehaviorEligibilityFactJunction,
    },
    AssetId,
};

#[derive(SystemParam)]
pub struct BehaviorTargetBiome<'w, 's> {
    terrain: Query<'w, 's, &'static TerrainIndex>,
    assets: Res<'w, Assets<TerrainAsset>>,
    chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
}

impl BehaviorTargetBiome<'_, '_> {
    pub fn at(&self, root: Entity, position: Vec3) -> Option<AssetId> {
        let point = position.xz();
        let (chunk, edits) = self
            .chunks
            .get(terrain_chunk_at(self.terrain.get(root).ok()?, point)?)
            .ok()?;
        let asset = self.assets.get(&chunk.asset)?;
        sample_terrain_biome(chunk, asset, edits, point).map(|biome| biome.id)
    }
}

struct ScoredBehaviorCandidate<T> {
    entry: T,
    priority: f32,
    score: f32,
}

pub struct BehaviorCandidateRanking<T> {
    minimum_score: f32,
    candidates: Vec<ScoredBehaviorCandidate<T>>,
}

impl<T: Copy> BehaviorCandidateRanking<T> {
    pub const fn new(minimum_score: f32) -> Self {
        Self {
            minimum_score,
            candidates: Vec::new(),
        }
    }

    pub fn consider(&mut self, entry: T, priority: f32, score: f32) {
        self.candidates.push(ScoredBehaviorCandidate {
            entry,
            priority,
            score,
        });
    }

    pub fn finish(mut self, random: impl FnOnce() -> u32) -> Option<(T, f32)> {
        if self.candidates.len() <= 1 {
            return self
                .candidates
                .first()
                .map(|candidate| (candidate.entry, candidate.score));
        }
        self.candidates.sort_by(|left, right| {
            right
                .priority
                .total_cmp(&left.priority)
                .then_with(|| right.score.total_cmp(&left.score))
        });
        let mut priority = self.candidates[0].priority;
        let mut best_score = 0.0_f32;
        let mut retained_in_group = false;
        let mut retained_higher_priority = false;
        self.candidates.retain(|candidate| {
            // The first entry at a new priority survives the group transition.
            if candidate.priority != priority {
                retained_higher_priority |= retained_in_group;
                priority = candidate.priority;
                best_score = 0.0;
                return true;
            }
            if retained_higher_priority
                || candidate.score < best_score
                || candidate.score <= self.minimum_score
                || candidate.score <= 0.0
            {
                return false;
            }
            retained_in_group = true;
            best_score = best_score.max(candidate.score);
            true
        });
        let first = self.candidates.first()?;
        let winner = if self
            .candidates
            .get(1)
            .is_some_and(|next| first.score == next.score)
        {
            &self.candidates[random() as usize % self.candidates.len()]
        } else {
            first
        };
        Some((winner.entry, winner.score))
    }
}

/// Applies authored type and qualifier conjunctions without copying live facts.
pub fn candidate_requirements_are_satisfied(
    requirements: &[BehaviorCandidateEligibilityRequirements],
    matches_type: impl Fn(BehaviorCandidateRole, AssetId) -> bool,
    satisfies_fact: impl Fn(BehaviorCandidateRole, &BehaviorCandidateEligibilityFact) -> bool,
) -> bool {
    requirements.iter().all(|requirement| {
        let matches_type = |required: &AssetId| matches_type(requirement.candidate_role, *required);
        let types_match = requirement.candidate_type_identifiers.is_empty()
            || match requirement.candidate_type_junction {
                BehaviorEligibilityFactJunction::All => requirement
                    .candidate_type_identifiers
                    .iter()
                    .all(matches_type),
                BehaviorEligibilityFactJunction::Any => requirement
                    .candidate_type_identifiers
                    .iter()
                    .any(matches_type),
            };
        let satisfies_fact = |fact: &BehaviorCandidateEligibilityFact| {
            satisfies_fact(requirement.candidate_role, fact)
        };
        types_match
            && (requirement.eligibility_facts.is_empty()
                || match requirement.eligibility_fact_junction {
                    BehaviorEligibilityFactJunction::All => {
                        requirement.eligibility_facts.iter().all(satisfies_fact)
                    }
                    BehaviorEligibilityFactJunction::Any => {
                        requirement.eligibility_facts.iter().any(satisfies_fact)
                    }
                })
    })
}

/// Native need-reduction score over borrowed actor facts; unresolved branches fail closed.
pub fn score_supported_behavior_task(
    task: &openzt2_game_data::behavior::document::BehaviorTask,
    outside_range_need_value: f32,
    target_distance: f32,
    target_biome: impl FnOnce() -> Option<AssetId>,
    source_need: impl Fn(AssetId) -> Option<(i32, bool)>,
    scalar_value: impl FnMut(openzt2_game_data::behavior::scalar::BehaviorScalarQ16) -> Option<i32>,
) -> Option<f32> {
    score_behavior_task(
        task,
        outside_range_need_value,
        target_distance,
        None,
        target_biome,
        source_need,
        scalar_value,
    )
}

/// As [`score_supported_behavior_task`], for an animal with its current good
/// and bad need points: an authored `needPointsGood` or `needPointsBad` is
/// the least the animal must have for the task to run, as `InviteReproduction`
/// waits for a contented animal and `BashFence` for an unhappy one.
pub fn score_behavior_task_with_need_points(
    task: &openzt2_game_data::behavior::document::BehaviorTask,
    outside_range_need_value: f32,
    target_distance: f32,
    need_points: (f32, f32),
    target_biome: impl FnOnce() -> Option<AssetId>,
    source_need: impl Fn(AssetId) -> Option<(i32, bool)>,
    scalar_value: impl FnMut(openzt2_game_data::behavior::scalar::BehaviorScalarQ16) -> Option<i32>,
) -> Option<f32> {
    score_behavior_task(
        task,
        outside_range_need_value,
        target_distance,
        Some(need_points),
        target_biome,
        source_need,
        scalar_value,
    )
}

fn score_behavior_task(
    task: &openzt2_game_data::behavior::document::BehaviorTask,
    outside_range_need_value: f32,
    target_distance: f32,
    need_points: Option<(f32, f32)>,
    target_biome: impl FnOnce() -> Option<AssetId>,
    source_need: impl Fn(AssetId) -> Option<(i32, bool)>,
    mut scalar_value: impl FnMut(openzt2_game_data::behavior::scalar::BehaviorScalarQ16) -> Option<i32>,
) -> Option<f32> {
    use openzt2_game_data::behavior::score::BehaviorScore;
    if let Some((good, bad)) = need_points {
        let met = task.scores.iter().all(|score| match score {
            BehaviorScore::AggregateGoodNeeds { weight } => good >= *weight,
            BehaviorScore::AggregateBadNeeds { weight } => bad >= *weight,
            _ => true,
        });
        if !met {
            return None;
        }
    }
    if task
        .scores
        .iter()
        .any(|score| matches!(score, BehaviorScore::LeaveZooPressure))
    {
        return None;
    }
    // fixedScore takes precedence over attribute and biome evaluation.
    if let Some(value) = task.scores.iter().find_map(|score| match score {
        BehaviorScore::Fixed(value) => Some(*value),
        _ => None,
    }) {
        return Some(value);
    }
    let distance_influenced = !task
        .scores
        .iter()
        .any(|entry| matches!(entry, BehaviorScore::DistanceInfluence(false)));
    let mut score = 0.0;
    let mut has_need_adjustment = false;
    let mut has_biome_map = false;
    for entry in &task.scores {
        match entry {
            BehaviorScore::AttributeValue { attribute, value } => {
                has_need_adjustment = true;
                let (deprivation_q16, triggered) = source_need(*attribute)?;
                let deprivation = if triggered {
                    deprivation_q16 as f32 / 65_536.0
                } else {
                    outside_range_need_value
                };
                // Concrete candidates sample rand; estimates use its minimum.
                let adjustment = scalar_value(*value)? as f32 / 65_536.0;
                score -= adjustment * (deprivation / 100.0);
            }
            // Applied once to the accumulated need score below.
            BehaviorScore::DistanceInfluence(_) => {}
            BehaviorScore::BiomeWeight { .. } => has_biome_map = true,
            // Need-point thresholds were checked above, when they are known.
            BehaviorScore::AggregateGoodNeeds { .. } | BehaviorScore::AggregateBadNeeds { .. }
                if need_points.is_some() => {}
            _ => return None,
        }
    }
    if distance_influenced {
        // Distance penalty: (max(distance, 1) - 1) / 1000 + 1.
        score /= (target_distance.max(1.0) - 1.0) / 1000.0 + 1.0;
    }
    if has_biome_map {
        // A concrete target on an unlisted biome and a self target use a unit factor.
        let factor = target_biome()
            .and_then(|target| {
                task.scores.iter().rev().find_map(|entry| match entry {
                    BehaviorScore::BiomeWeight { biome, weight } if *biome == target => {
                        Some(*weight)
                    }
                    _ => None,
                })
            })
            .unwrap_or(1.0);
        score *= factor;
    }
    ((has_need_adjustment || has_biome_map) && score.is_finite()).then_some(score)
}

#[cfg(test)]
mod inherited_species_task_compatibility_tests;

#[cfg(test)]
mod need_point_threshold_tests {
    use super::*;
    use openzt2_game_data::behavior::document::{BehaviorTask, BehaviorTaskActionPhase};
    use openzt2_game_data::behavior::scalar::BehaviorScalarQ16;
    use openzt2_game_data::behavior::score::BehaviorScore;

    fn task(scores: Vec<BehaviorScore>) -> BehaviorTask {
        BehaviorTask {
            id: AssetId::from_key("task"),
            name: "task".to_owned(),
            priority: None,
            task_delay_seconds: None,
            reservation_tag: None,
            sensor_tag: None,
            subjects: Vec::new(),
            targets: Vec::new(),
            objects: Vec::new(),
            candidate_eligibility_requirements: Vec::new(),
            scores,
            execution: BehaviorTaskActionPhase::Supported(Vec::new()),
            completion: BehaviorTaskActionPhase::Supported(Vec::new()),
            failure: BehaviorTaskActionPhase::Supported(Vec::new()),
        }
    }

    fn score(task: &BehaviorTask, need_points: (f32, f32)) -> Option<f32> {
        score_behavior_task_with_need_points(
            task,
            0.0,
            1.0,
            need_points,
            || None,
            |_| Some((50 << 16, true)),
            |value| match value {
                BehaviorScalarQ16::FixedQ16(value) => Some(value),
                _ => None,
            },
        )
    }

    #[test]
    fn a_fence_bash_waits_for_its_bad_need_points() {
        // As the shipped BashFence: needPointsBad="30" fixedScore="100".
        let bash = task(vec![
            BehaviorScore::AggregateBadNeeds { weight: 30.0 },
            BehaviorScore::Fixed(100.0),
        ]);
        assert_eq!(score(&bash, (0.0, 29.8)), None);
        assert_eq!(score(&bash, (0.0, 30.0)), Some(100.0));
    }

    #[test]
    fn a_contented_task_waits_for_its_good_need_points_then_scores_its_needs() {
        // As InviteReproduction: needPointsGood with reproduction="-40".
        let invite = task(vec![
            BehaviorScore::AggregateGoodNeeds { weight: 50.0 },
            BehaviorScore::AttributeValue {
                attribute: AssetId::from_key("reproduction"),
                value: BehaviorScalarQ16::FixedQ16(-40 << 16),
            },
        ]);
        assert_eq!(score(&invite, (49.0, 0.0)), None);
        assert_eq!(score(&invite, (50.0, 0.0)), Some(20.0));
        // Without need points, as for guests, the threshold stays unsupported.
        assert_eq!(
            score_supported_behavior_task(
                &invite,
                0.0,
                1.0,
                || None,
                |_| Some((50 << 16, true)),
                |_| { Some(-40 << 16) }
            ),
            None
        );
    }

    #[test]
    fn a_concrete_target_applies_its_biome_after_the_distance_penalty() {
        let desert = AssetId::from_key("desert");
        let task = task(vec![
            BehaviorScore::AttributeValue {
                attribute: AssetId::from_key("exercise"),
                value: BehaviorScalarQ16::FixedQ16(-40 << 16),
            },
            BehaviorScore::BiomeWeight {
                biome: desert,
                weight: 1.5,
            },
        ]);
        let score = |biome| {
            score_supported_behavior_task(
                &task,
                0.0,
                1001.0,
                || biome,
                |_| Some((50 << 16, true)),
                |_| Some(-40 << 16),
            )
        };
        assert_eq!(score(Some(desert)), Some(15.0));
        assert_eq!(score(Some(AssetId::from_key("tundra"))), Some(10.0));
        assert_eq!(score(None), Some(10.0));
    }
}

#[cfg(test)]
mod candidate_ranking_tests {
    use super::BehaviorCandidateRanking;

    #[test]
    fn score_difference_does_not_consume_a_draw() {
        let mut ranking = BehaviorCandidateRanking::new(0.0);
        ranking.consider("sleep", 0.0, 35.0);
        ranking.consider("lie", 0.0, 40.0);
        assert_eq!(
            ranking.finish(|| panic!("unexpected draw")),
            Some(("lie", 40.0))
        );
    }

    #[test]
    fn a_sole_candidate_skips_the_minimum_and_the_draw() {
        let mut ranking = BehaviorCandidateRanking::new(10.0);
        ranking.consider("single", 0.0, 1.0);
        assert_eq!(
            ranking.finish(|| panic!("unexpected draw")),
            Some(("single", 1.0))
        );
    }

    #[test]
    fn the_tied_branch_samples_the_remaining_candidates() {
        let mut ranking = BehaviorCandidateRanking::new(0.0);
        ranking.consider("first", 0.0, 40.0);
        ranking.consider("pruned", 0.0, 20.0);
        ranking.consider("second", 0.0, 40.0);
        assert_eq!(ranking.finish(|| 3), Some(("second", 40.0)));
    }
}
