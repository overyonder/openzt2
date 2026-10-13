use openzt2_world_state::simulation_time::deterministic_random_stream;
use openzt2_world_state::simulation_time::simulation_clock_types;
use openzt2_world_state::simulation_time::simulation_control_queries;
mod simulation_control_action_routing;
mod simulation_control_application;
use openzt2_world_state::simulation_time::simulation_control_types;
mod simulation_control_ui_presentation;
pub mod simulation_time_world_lifecycle;
use openzt2_world_state::simulation_time::zoo_calendar_calculation;
mod zoo_clock_advancement;
mod zoo_date_ui_presentation;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;

pub struct SimulationTimePlugin;

use simulation_clock_types::{ZooCalendar, ZooDayAdvanced, ZooYearAdvanced};
use simulation_control_types::{
    SetSimulationSpeed, SimulationControl, SimulationControlChanged, SimulationPauseRequest,
};

impl Plugin for SimulationTimePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, simulation_time_world_lifecycle::initialize_simulation_time_from_selected_world.in_set(openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadPreparationSet::InitializeClock).in_set(GameSet::Intent).run_if(in_state(GamePhase::Loading)));
        app.add_message::<SetSimulationSpeed>()
            .add_message::<SimulationPauseRequest>()
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
                    .after(openzt2_interface_state::shell::ShellNavigationAfterSettings)
                    .run_if(resource_exists::<SimulationControl>),
            )
            .add_systems(
                Update,
                simulation_control_action_routing::route_pause_game_action_to_simulation_control_requests
                    .in_set(GameSet::Intent)
                    .before(simulation_control_application::apply_requested_simulation_control_changes),
            )
            .add_systems(
                Update,
                simulation_control_action_routing::route_authored_ui_simulation_actions_to_simulation_control_requests
                    .in_set(openzt2_interface_state::ui::UiSet::Routing),
            )
            .add_systems(
                Update,
                simulation_control_application::reconcile_authored_ui_temporary_pause_holds
                    .after(openzt2_interface_state::ui::UiSet::Notifications)
                    .in_set(GameSet::Ui)
                    .run_if(resource_exists::<SimulationControl>),
            )
            .add_systems(
                Update,
                (
                    zoo_date_ui_presentation::project_authoritative_zoo_calendar_into_authored_zoo_date_text
                        .run_if(resource_exists::<ZooCalendar>),
                    simulation_control_ui_presentation::project_simulation_pause_state_into_authored_paused_node_visibility
                        .after(simulation_control_application::reconcile_authored_ui_temporary_pause_holds)
                        .run_if(resource_exists::<SimulationControl>),
                )
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                simulation_control_ui_presentation::project_simulation_pause_state_into_authored_pause_toggle_selection
                    .in_set(openzt2_interface_state::ui::UiSet::DomainProjection)
                    .run_if(resource_exists::<SimulationControl>)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                zoo_clock_advancement::advance_authoritative_zoo_clock_and_calendar
                    .in_set(FixedGameSet::Clock)
                    .run_if(in_state(GamePhase::InGame))
                    .run_if(simulation_control_queries::simulation_is_running),
            )
            .add_systems(
                OnExit(GamePhase::InGame),
                simulation_time_world_lifecycle::remove_simulation_time_after_leaving_world,
            );
    }
}

#[cfg(test)]
mod zoo_calendar_and_clock_tests;
