use std::{num::NonZeroU32, time::Duration};

use bevy::{
    prelude::*,
    time::{Fixed, Virtual},
};
use openzt2_world_state::world_spawn::selected_world_document_handles::SelectedWorldDocumentHandles;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::{
    WorldDefinitions, WorldDefinitionsView,
};
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_hydration_types::WorldHydration;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailure;

use super::{
    deterministic_random_stream::{derive_zoo_seed_from_scenario_and_profile, ZooSeed},
    simulation_clock_types::{ZooCalendar, ZooClock, ZooDay, ZooDayCadence, ZooMonth},
    simulation_control_types::{SimulationControl, SimulationPaused, SimulationSpeedTier},
    zoo_calendar_calculation::calculate_zoo_calendar_for_absolute_day,
};

/// The most game time one frame advances; slower frames slow the game.
const MAXIMUM_FRAME_CATCH_UP: Duration = Duration::from_millis(250);

pub fn zoo_day_cadence_from_world_definitions(
    definitions: WorldDefinitionsView<'_>,
) -> Option<ZooDayCadence> {
    if let Some(timekeeper) = definitions.authored_timekeeper_policy() {
        ZooDayCadence::new(timekeeper.simulation.interval, timekeeper.time_scale)
    } else {
        NonZeroU32::new(definitions.timing().ticks_per_day).map(ZooDayCadence::from_ticks_per_day)
    }
}

pub fn initialize_simulation_time_from_selected_world(
    mut commands: Commands,
    selected_worlds: Query<
        (
            Entity,
            &SelectedWorldIdentity,
            &SelectedWorldDocumentHandles,
        ),
        (
            With<WorldHydration>,
            With<WorldRoot>,
            Without<ZooClock>,
            Without<WorldLoadFailure>,
        ),
    >,
    scenario_documents: Res<Assets<WorldScenarioDocumentAsset>>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut fixed_time: ResMut<Time<Fixed>>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    let Ok((root, selected_world, documents)) = selected_worlds.single() else {
        return;
    };
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        return;
    };
    let timing = definitions.timing();
    let calendar_policy = &timing.calendar;
    let Some(starting_zoo) = scenario_documents
        .get(&documents.start)
        .and_then(|document| document.document.find_starting_zoo(selected_world.start))
    else {
        commands
            .entity(root)
            .insert_if_new(WorldLoadFailure::MissingScenario);
        return;
    };
    let absolute_day = starting_zoo.absolute_day;
    let tick = starting_zoo.start_tick;
    let timekeeper = definitions.authored_timekeeper_policy();
    let Some((step, cadence, mut clock)) = definitions
        .simulation_timestep()
        .zip(zoo_day_cadence_from_world_definitions(definitions))
        .and_then(|(step, cadence)| {
            cadence
                .clock_at(tick, Default::default())
                .map(|clock| (step, cadence, clock))
        })
    else {
        commands
            .entity(root)
            .insert_if_new(WorldLoadFailure::InvalidRecord(0));
        return;
    };
    clock.absolute_day = absolute_day;
    let calculated_calendar =
        calculate_zoo_calendar_for_absolute_day(calendar_policy, absolute_day);
    let imported_calendar = ZooCalendar {
        year: starting_zoo.calendar[0],
        month: starting_zoo.calendar[1] as u8,
        day: starting_zoo.calendar[2] as u8,
    };
    if calculated_calendar != imported_calendar {
        warn!(start = ?selected_world.start, ?calculated_calendar, ?imported_calendar, absolute_day, "starting-zoo calendar does not match the native calendar policy");
        commands
            .entity(root)
            .insert_if_new(WorldLoadFailure::InvalidRecord(0));
        return;
    }
    let zoo_seed = starting_zoo.imported_seed.map_or_else(
        || {
            derive_zoo_seed_from_scenario_and_profile(
                selected_world.requested,
                selected_world.profile,
            )
        },
        ZooSeed,
    );
    fixed_time.set_timestep(step);
    virtual_time.set_max_delta(MAXIMUM_FRAME_CATCH_UP);
    let mut world = commands.entity(root);
    world.insert((
        clock,
        cadence,
        calculated_calendar,
        ZooDay(absolute_day),
        ZooMonth::from(calculated_calendar),
        SimulationControl::new(false),
        SimulationPaused::default(),
        SimulationSpeedTier::default(),
        zoo_seed,
    ));
    if let Some(policy) = timekeeper {
        world.insert((
            super::timekeeper_cadence::ZooSlowTickTimer::new(policy.slow.interval),
            super::timekeeper_cadence::ZooHeartbeatTimer::new(
                policy.heartbeat.interval,
                policy.heartbeat.initially_paused,
            ),
        ));
    }
}
