use openzt2_world_state::simulation_time::deterministic_random_stream;
use openzt2_world_state::simulation_time::simulation_clock_types;
use openzt2_world_state::simulation_time::simulation_control_queries;
mod simulation_control_action_routing;
mod simulation_control_application;
mod simulation_speed_override;
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
use openzt2_zoo_state::environment::environment_state_types::ZooTickAdvanced;

pub struct SimulationTimePlugin;

use simulation_clock_types::{ZooDayAdvanced, ZooYearAdvanced};
use simulation_control_types::{
    SetSimulationSpeed, SimulationControlChanged, SimulationPauseRequest,
};

impl Plugin for SimulationTimePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<simulation_speed_override::SimulationSpeedOverride>();
        app.add_systems(Update, simulation_time_world_lifecycle::initialize_simulation_time_from_selected_world.in_set(openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadPreparationSet::InitializeClock).in_set(GameSet::Intent).run_if(in_state(GamePhase::Loading)));
        app.add_message::<SetSimulationSpeed>()
            .add_message::<SimulationPauseRequest>()
            .add_message::<SimulationControlChanged>()
            .add_message::<ZooDayAdvanced>()
            .add_message::<ZooTickAdvanced>()
            .add_message::<ZooYearAdvanced>()
            .add_systems(
                Update,
                simulation_control_application::apply_requested_simulation_control_changes
                    .in_set(GameSet::Intent)
                    .after(simulation_time_world_lifecycle::initialize_simulation_time_from_selected_world)
                    .after(openzt2_interface_state::shell::ShellNavigationAfterSettings)
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
                PostUpdate,
                simulation_control_application::project_active_simulation_pause_claims
                    .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate)
            )
            .add_systems(
                Update,
                (
                    zoo_date_ui_presentation::project_authoritative_zoo_calendar_into_authored_zoo_date_text,
                    simulation_control_ui_presentation::project_simulation_pause_state_into_authored_paused_node_visibility,
                )
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                simulation_control_ui_presentation::project_simulation_pause_state_into_authored_pause_toggle_selection
                    .in_set(openzt2_interface_state::ui::UiSet::DomainProjection)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                zoo_clock_advancement::advance_authoritative_zoo_clock_and_calendar
                    .in_set(FixedGameSet::Clock)
                    .run_if(in_state(GamePhase::InGame))
                    .run_if(simulation_control_queries::simulation_is_running),
            );
    }
}

#[cfg(test)]
mod zoo_calendar_and_clock_tests;
