//! Native need-reduction scoring for the supported concrete candidates.

use super::candidate_facts::AnimalCandidateFactsItem;
use crate::plugins::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;
use openzt2_game_data::behavior::document::BehaviorTask;

pub(super) fn score_animal_task(
    task: &BehaviorTask,
    facts: &AnimalCandidateFactsItem<'_, '_>,
    outside_range_need_value: f32,
    target_distance: f32,
    random: &mut BehaviorRandomStream,
) -> Option<f32> {
    crate::plugins::animal_behavior::behavior_candidate_selection::score_supported_behavior_task(
        task,
        outside_range_need_value,
        target_distance,
        |attribute| facts.source_need(attribute),
        |value| value.sample_q16(|upper| random.range_u32(upper).unwrap_or_default()),
    )
}
