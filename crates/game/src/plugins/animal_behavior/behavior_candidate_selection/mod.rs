//! Shared candidate ranking over borrowed canonical task records and live facts.

use openzt2_game_data::{
    behavior::eligibility::{
        BehaviorCandidateEligibilityFact, BehaviorCandidateEligibilityRequirements,
        BehaviorCandidateRole, BehaviorEligibilityFactJunction,
    },
    AssetId,
};

/// Ranks candidates by priority and score, retaining tied winners.
pub(crate) struct BehaviorCandidateRanking<T> {
    minimum_score: f32,
    selected: Option<T>,
    sole_candidate: Option<T>,
    candidate_count: usize,
    best_priority: f32,
    best_score: f32,
    tied_candidates: u32,
}

impl<T: Copy> BehaviorCandidateRanking<T> {
    pub(crate) fn new(minimum_score: f32) -> Self {
        Self {
            minimum_score,
            selected: None,
            sole_candidate: None,
            candidate_count: 0,
            best_priority: f32::NEG_INFINITY,
            best_score: f32::NEG_INFINITY,
            tied_candidates: 0,
        }
    }

    pub(crate) fn consider(
        &mut self,
        candidate: T,
        priority: f32,
        score: f32,
        mut random: impl FnMut(u32) -> Option<u32>,
    ) {
        self.candidate_count += 1;
        self.sole_candidate = Some(candidate);
        if score <= self.minimum_score {
            return;
        }
        if priority < self.best_priority
            || (priority == self.best_priority && score < self.best_score)
        {
            return;
        }
        if priority > self.best_priority || score > self.best_score {
            self.best_priority = priority;
            self.best_score = score;
            self.tied_candidates = 1;
        } else {
            self.tied_candidates = self.tied_candidates.saturating_add(1);
            if random(self.tied_candidates) != Some(0) {
                return;
            }
        }
        self.selected = Some(candidate);
    }

    pub(crate) fn finish(self, allow_single_candidate_promotion: bool) -> Option<T> {
        if allow_single_candidate_promotion && self.candidate_count == 1 {
            self.sole_candidate
        } else {
            self.selected
        }
    }
}

/// Applies authored type and qualifier conjunctions without copying live facts.
pub(crate) fn candidate_requirements_are_satisfied(
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
pub(crate) fn score_supported_behavior_task(
    task: &openzt2_game_data::behavior::document::BehaviorTask,
    outside_range_need_value: f32,
    target_distance: f32,
    source_need: impl Fn(AssetId) -> Option<(i32, bool)>,
    mut scalar_value: impl FnMut(openzt2_game_data::behavior::scalar::BehaviorScalarQ16) -> Option<i32>,
) -> Option<f32> {
    use openzt2_game_data::behavior::score::BehaviorScore;
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
            // Current need-point branches and biome multiplier have separate owners.
            // Until those inputs are available, do not silently treat them as neutral.
            _ => return None,
        }
    }
    if distance_influenced {
        // Distance penalty: (max(distance, 1) - 1) / 1000 + 1.
        score /= (target_distance.max(1.0) - 1.0) / 1000.0 + 1.0;
    }
    (has_need_adjustment && score.is_finite()).then_some(score)
}
