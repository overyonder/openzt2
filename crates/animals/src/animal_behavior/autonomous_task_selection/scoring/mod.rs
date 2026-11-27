//! Native need-reduction scoring for the supported concrete candidates.

use super::candidate_facts::AnimalCandidateFactsItem;
use openzt2_game_data::behavior::document::BehaviorTask;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;

pub(super) fn score_animal_task(
    task: &BehaviorTask,
    facts: &AnimalCandidateFactsItem<'_, '_>,
    time: (u64, f64),
    outside_range_need_value: f32,
    target_distance: f32,
    target_biome: impl FnOnce() -> Option<AssetId>,
    random: &mut BehaviorRandomStream,
) -> Option<f32> {
    openzt2_zoo_state::animal_behavior::behavior_candidate_selection::score_behavior_task_with_need_points(
        task,
        outside_range_need_value,
        target_distance,
        facts.need_points(),
        target_biome,
        |attribute| {
            if attribute == AssetId::from_key("lifespan") {
                facts.source_need(attribute)
                    .or_else(|| facts.age_q16(time.0, time.1).map(|age| (age, true)))
            } else {
                facts.source_need(attribute)
            }
        },
        |value| value.sample_q16(|upper| random.range_u32(upper).unwrap_or_default()),
    )
}
