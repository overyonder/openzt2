use openzt2_game_data::world_definitions::catalogue_and_progression::research_and_unlock_definition_types::ResearchDuration;
use crate::plugins::simulation_time::deterministic_random_stream::{DeterministicRng, RngDomain, ZooSeed};
use crate::plugins::world_spawn::persistent_id_types::PersistentId;

pub(crate) fn resolve_research_duration_to_simulation_ticks(
    duration: ResearchDuration,
    fixed_hz: u16,
    seed: ZooSeed,
    identifier: PersistentId,
) -> u64 {
    match duration {
        ResearchDuration::Ticks(ticks) => ticks.max(1),
        ResearchDuration::Nanoseconds(nanoseconds) => {
            u64::try_from((u128::from(nanoseconds) * u128::from(fixed_hz)).div_ceil(1_000_000_000))
                .unwrap_or(u64::MAX)
                .max(1)
        }
        ResearchDuration::RandomDefault => {
            let mut random = DeterministicRng::from_entity(seed, identifier, RngDomain::Research);
            (u64::from(25 + random.next_u32() % 30) * u64::from(fixed_hz)).max(1)
        }
    }
}
