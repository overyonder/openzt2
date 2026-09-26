use bevy::{prelude::*, time::Virtual};

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::simulation_control_types::{
    SetSimulationPaused, SetSimulationSpeed, SimulationControl, SimulationControlChanged,
};

pub(in crate::plugins::simulation_time) fn apply_requested_simulation_control_changes(
    mut speed_requests: MessageReader<SetSimulationSpeed>,
    mut pause_requests: MessageReader<SetSimulationPaused>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut simulation_control: ResMut<SimulationControl>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut control_changes: MessageWriter<SimulationControlChanged>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let speed_multipliers = &definitions.timing().speed_multipliers;
    if speed_multipliers.is_empty()
        || usize::from(simulation_control.speed_tier) >= speed_multipliers.len()
    {
        return;
    }
    let previous_control = *simulation_control;

    for request in speed_requests.read() {
        accept_requested_simulation_speed_tier(
            &mut simulation_control,
            request.tier,
            speed_multipliers.len(),
        );
    }
    for request in pause_requests.read() {
        set_simulation_paused(&mut simulation_control, request.0);
    }

    let speed_multiplier = &speed_multipliers[usize::from(simulation_control.speed_tier)];
    let relative_speed = speed_multiplier.numerator as f32 / speed_multiplier.denominator as f32;
    if virtual_time.relative_speed() != relative_speed || virtual_time.is_paused() {
        virtual_time.set_relative_speed(relative_speed);
        // Domain simulation sets use `simulation_is_running`. Bevy virtual time
        // must continue driving player-owned fixed transactions while paused.
        virtual_time.unpause();
    }
    if *simulation_control == previous_control {
        return;
    }
    control_changes.write(SimulationControlChanged {
        speed_tier: simulation_control.speed_tier,
        paused: simulation_control.paused,
    });
}

pub(crate) fn simulation_is_running(simulation_control: Option<Res<SimulationControl>>) -> bool {
    simulation_control.is_some_and(|control| !control.paused)
}

pub(crate) fn simulation_is_paused(simulation_control: Option<Res<SimulationControl>>) -> bool {
    simulation_control.is_some_and(|control| control.paused)
}

pub(crate) fn accept_requested_simulation_speed_tier(
    simulation_control: &mut SimulationControl,
    requested_speed_tier: u8,
    speed_tier_count: usize,
) -> bool {
    if usize::from(requested_speed_tier) >= speed_tier_count
        || simulation_control.speed_tier == requested_speed_tier
    {
        return false;
    }
    simulation_control.speed_tier = requested_speed_tier;
    true
}

pub(crate) fn set_simulation_paused(
    simulation_control: &mut SimulationControl,
    paused: bool,
) -> bool {
    if simulation_control.paused == paused {
        return false;
    }
    simulation_control.paused = paused;
    true
}
