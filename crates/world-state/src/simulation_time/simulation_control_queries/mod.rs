use super::simulation_control_types::SimulationControl;
use crate::world_spawn::world_membership_types::WorldRoot;
use bevy::prelude::*;

pub fn simulation_is_running(roots: Query<&SimulationControl, With<WorldRoot>>) -> bool {
    roots.single().is_ok_and(|control| !control.paused)
}

pub fn simulation_is_paused(roots: Query<&SimulationControl, With<WorldRoot>>) -> bool {
    roots.single().is_ok_and(|control| control.paused)
}
