pub(crate) mod deterministic_random_stream;
pub(crate) mod simulation_clock_types;
mod simulation_control_action_routing;
pub(crate) mod simulation_control_application;
pub(crate) mod simulation_control_types;
pub(crate) mod simulation_control_ui_presentation;
pub(crate) mod simulation_time_world_lifecycle;
mod zoo_calendar_calculation;
mod zoo_clock_advancement;
mod zoo_date_ui_presentation;

use bevy::prelude::*;

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::{FixedGameSet, GameSet};

pub struct SimulationTimePlugin;

use simulation_clock_types::{ZooCalendar, ZooDayAdvanced, ZooYearAdvanced};
use simulation_control_types::{
    SetSimulationPaused, SetSimulationSpeed, SimulationControl, SimulationControlChanged,
};

impl Plugin for SimulationTimePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SetSimulationSpeed>()
            .add_message::<SetSimulationPaused>()
            .add_message::<SimulationControlChanged>()
            .add_message::<ZooDayAdvanced>()
            .add_message::<ZooYearAdvanced>()
            .configure_sets(
                FixedUpdate,
                (
                    FixedGameSet::Clock,
                    FixedGameSet::Think,
                    FixedGameSet::Navigate,
                    FixedGameSet::Act,
                    FixedGameSet::Economy,
                    FixedGameSet::Cleanup,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                simulation_control_application::apply_requested_simulation_control_changes
                    .in_set(GameSet::Intent)
                    .after(simulation_time_world_lifecycle::initialize_simulation_time_from_selected_world)
                    .run_if(resource_exists::<SimulationControl>),
            )
            .add_systems(
                Update,
                (
                    simulation_control_action_routing::route_pause_game_action_to_simulation_control_requests,
                    simulation_control_action_routing::route_authored_ui_simulation_actions_to_simulation_control_requests,
                )
                    .in_set(GameSet::Intent)
                    .before(simulation_control_application::apply_requested_simulation_control_changes),
            )
            .add_systems(
                Update,
                (
                    zoo_date_ui_presentation::project_authoritative_zoo_calendar_into_authored_zoo_date_text
                        .run_if(resource_exists::<ZooCalendar>),
                    simulation_control_ui_presentation::project_simulation_pause_state_into_authored_paused_node_visibility
                        .run_if(resource_exists::<SimulationControl>),
                )
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                simulation_control_ui_presentation::project_simulation_pause_state_into_authored_pause_toggle_selection
                    .in_set(crate::plugins::ui::UiSet::DomainProjection)
                    .run_if(resource_exists::<SimulationControl>)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                zoo_clock_advancement::advance_authoritative_zoo_clock_and_calendar
                    .in_set(FixedGameSet::Clock)
                    .run_if(in_state(GamePhase::InGame))
                    .run_if(simulation_control_application::simulation_is_running),
            )
            .add_systems(
                OnExit(GamePhase::InGame),
                simulation_time_world_lifecycle::remove_simulation_time_after_leaving_world,
            );
    }
}

#[cfg(test)]
mod deterministic_random_stream_tests;

#[cfg(test)]
mod zoo_calendar_and_clock_tests;
