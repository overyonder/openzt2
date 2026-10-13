use super::simulation_control_types::SimulationControl;
use bevy::prelude::*;

pub fn simulation_is_running(simulation_control: Option<Res<SimulationControl>>) -> bool {
    simulation_control.is_some_and(|control| !control.paused)
}

pub fn simulation_is_paused(simulation_control: Option<Res<SimulationControl>>) -> bool {
    simulation_control.is_some_and(|control| control.paused)
}
