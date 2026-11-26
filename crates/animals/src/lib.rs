pub mod animal_behavior;
pub mod animal_health;
pub mod animal_lifecycle;
pub mod animal_welfare;
pub mod feeding;

fn zoo_day_cadence(
    definitions: openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
) -> openzt2_world_state::simulation_time::simulation_clock_types::ZooDayCadence {
    use openzt2_world_state::simulation_time::simulation_clock_types::ZooDayCadence;

    if let Some(policy) = definitions.authored_timekeeper_policy() {
        ZooDayCadence::new(policy.simulation.interval, policy.time_scale)
            .expect("simulation timing is valid")
    } else {
        ZooDayCadence::from_ticks_per_day(
            std::num::NonZeroU32::new(definitions.timing().ticks_per_day)
                .expect("simulation timing is valid"),
        )
    }
}
