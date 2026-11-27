use std::time::Duration;

use openzt2_game_data::world_definitions::catalogue_and_progression::research_and_unlock_definition_types::ResearchDuration;
use openzt2_world_state::simulation_time::deterministic_random_stream::{DeterministicRng, RngDomain, ZooSeed};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;

use super::research_types::fixed_tick_duration;

pub fn resolve_research_duration(
    duration: ResearchDuration,
    fixed_step: Duration,
    seed: ZooSeed,
    identifier: PersistentId,
) -> Duration {
    match duration {
        ResearchDuration::Ticks(ticks) => fixed_tick_duration(fixed_step, ticks.max(1)),
        ResearchDuration::Nanoseconds(nanoseconds) => {
            Duration::from_nanos(nanoseconds).max(fixed_step)
        }
        ResearchDuration::RandomDefault => {
            let mut random = DeterministicRng::from_entity(seed, identifier, RngDomain::Research);
            Duration::from_secs(u64::from(25 + random.next_u32() % 30)).max(fixed_step)
        }
    }
}
