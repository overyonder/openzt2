use bevy::{
    prelude::*,
    time::{Fixed, Virtual},
};

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity;

use super::{
    deterministic_random_stream::{derive_zoo_seed_from_scenario_and_profile, ZooSeed},
    simulation_clock_types::{ZooCalendar, ZooClock},
    simulation_control_types::SimulationControl,
    zoo_calendar_calculation::calculate_zoo_calendar_for_absolute_day,
};

pub(crate) fn initialize_simulation_time_from_selected_world(
    mut commands: Commands,
    selected_worlds: Query<&SelectedWorldIdentity>,
    existing_zoo_clock: Option<Res<ZooClock>>,
    scenario_documents: Res<Assets<WorldScenarioDocumentAsset>>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut fixed_time: ResMut<Time<Fixed>>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    if existing_zoo_clock.is_some() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        if !selected_worlds.is_empty() {
            warn!(
                loaded = definition_assets.len(),
                "world time initialization is waiting for the authoritative definitions asset"
            );
        }
        return;
    };
    let timing = definitions.timing();
    let fixed_updates_per_second = f64::from(timing.fixed_hz);
    fixed_time.set_timestep_hz(fixed_updates_per_second);
    virtual_time.set_max_delta(std::time::Duration::from_secs_f64(
        2.0 / fixed_updates_per_second,
    ));
    let calendar_policy = &timing.calendar;

    for selected_world in &selected_worlds {
        let Some(starting_zoo) = scenario_documents
            .iter()
            .find_map(|(_, document)| document.document.find_starting_zoo(selected_world.start))
        else {
            warn!(start = ?selected_world.start, loaded = scenario_documents.len(), "selected world has no starting-zoo time record");
            continue;
        };
        let absolute_day = starting_zoo.absolute_day;
        let tick = starting_zoo.start_tick;
        let tick_in_day = (tick % u64::from(timing.ticks_per_day)) as u32;
        let calculated_calendar =
            calculate_zoo_calendar_for_absolute_day(calendar_policy, absolute_day);
        let imported_calendar = ZooCalendar {
            year: starting_zoo.calendar[0],
            month: starting_zoo.calendar[1] as u8,
            day: starting_zoo.calendar[2] as u8,
        };
        if calculated_calendar != imported_calendar {
            warn!(start = ?selected_world.start, ?calculated_calendar, ?imported_calendar, absolute_day, "starting-zoo calendar does not match the native calendar policy");
            continue;
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
        commands.insert_resource(ZooClock {
            tick,
            absolute_day,
            tick_in_day,
        });
        commands.insert_resource(calculated_calendar);
        commands.insert_resource(SimulationControl {
            speed_tier: 0,
            paused: false,
        });
        commands.insert_resource(zoo_seed);
        break;
    }
}

pub(in crate::plugins::simulation_time) fn remove_simulation_time_after_leaving_world(
    mut commands: Commands,
) {
    commands.remove_resource::<ZooClock>();
    commands.remove_resource::<ZooCalendar>();
    commands.remove_resource::<SimulationControl>();
    commands.remove_resource::<ZooSeed>();
}
