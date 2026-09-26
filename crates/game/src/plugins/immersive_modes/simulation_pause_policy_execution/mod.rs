use bevy::prelude::*;

use crate::plugins::simulation_time::simulation_control_types::{
    SetSimulationPaused, SimulationControl,
};

use super::immersive_mode_policy_types::{PauseSimulationWhileActive, RestoreSimulationPause};

/// Applies authored immersive-mode pause policy through the simulation clock's
/// ordinary typed request boundary.
pub(super) fn pause_simulation_while_requested_immersive_mode_is_active_and_restore_previous_state(
    newly_pausing_controllers: Query<Entity, Added<PauseSimulationWhileActive>>,
    previous_simulation_pause_states: Query<&RestoreSimulationPause>,
    simulation_control: Option<Res<SimulationControl>>,
    mut removed_pause_policy_components: RemovedComponents<PauseSimulationWhileActive>,
    mut set_simulation_paused_requests: MessageWriter<SetSimulationPaused>,
    mut commands: Commands,
) {
    for controller_entity in &newly_pausing_controllers {
        commands
            .entity(controller_entity)
            .insert(RestoreSimulationPause(
                simulation_control
                    .as_deref()
                    .is_some_and(|simulation_control| simulation_control.paused),
            ));
        set_simulation_paused_requests.write(SetSimulationPaused(true));
    }
    for controller_entity in removed_pause_policy_components.read() {
        if let Ok(previous_simulation_pause_state) =
            previous_simulation_pause_states.get(controller_entity)
        {
            set_simulation_paused_requests
                .write(SetSimulationPaused(previous_simulation_pause_state.0));
            commands
                .entity(controller_entity)
                .remove::<RestoreSimulationPause>();
        }
    }
}
