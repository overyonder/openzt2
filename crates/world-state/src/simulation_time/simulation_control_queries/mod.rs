use bevy::prelude::*;

use crate::world_spawn::world_membership_types::WorldRoot;

use super::simulation_control_types::SimulationPaused;

pub fn simulation_is_running(roots: Query<&SimulationPaused, With<WorldRoot>>) -> bool {
    roots.single().is_ok_and(|paused| !paused.0)
}

pub fn simulation_is_paused(roots: Query<&SimulationPaused, With<WorldRoot>>) -> bool {
    roots.single().is_ok_and(|paused| paused.0)
}
