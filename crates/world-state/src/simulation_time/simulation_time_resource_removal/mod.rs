use super::{
    deterministic_random_stream::ZooSeed,
    simulation_clock_types::{ZooCalendar, ZooClock},
    simulation_control_types::SimulationControl,
};
use bevy::prelude::*;

pub fn remove_world_simulation_time_resources(commands: &mut Commands) {
    commands.remove_resource::<ZooClock>();
    commands.remove_resource::<ZooCalendar>();
    commands.remove_resource::<SimulationControl>();
    commands.remove_resource::<ZooSeed>();
}
