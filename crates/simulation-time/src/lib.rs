use openzt2_world_state::simulation_time::{
    deterministic_random_stream, simulation_clock_types, simulation_control_queries,
    simulation_control_types, zoo_calendar_calculation,
};

mod simulation_control_action_routing;
mod simulation_control_application;
mod simulation_control_ui_presentation;
mod simulation_speed_override;
pub mod simulation_time_world_lifecycle;
mod timekeeper_cadence;
mod zoo_clock_advancement;
mod zoo_date_ui_presentation;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_interface_state::{input::input_types::ActionRequest, ui::UiSet};
use openzt2_world_state::{
    application_schedule::{FixedGameSet, GameSet},
    world_spawn::world_load_completion_marker::WorldLoadPreparationSet,
};
use openzt2_zoo_state::environment::environment_state_types::ZooTickAdvanced;

use simulation_clock_types::{
    ZooDayAdvanced, ZooHeartbeatElapsed, ZooSlowTickElapsed, ZooYearAdvanced,
};
use simulation_control_types::{
    SetSimulationSpeed, SimulationControlChanged, SimulationPauseRequest,
};

pub struct SimulationTimePlugin;

impl Plugin for SimulationTimePlugin {
    fn build(&self, app: &mut App) {
        use simulation_control_action_routing::{
            route_authored_ui_simulation_actions_to_simulation_control_requests,
            route_pause_game_action_to_simulation_control_requests,
        };
        use simulation_control_application::{
            apply_simulation_control_requests, pause_claim_inputs_changed,
            project_active_simulation_pause_claims, project_simulation_speed,
            simulation_speed_inputs_changed,
        };
        use simulation_time_world_lifecycle::initialize_simulation_time_from_selected_world;

        app.init_resource::<simulation_speed_override::SimulationSpeedOverride>()
            .add_message::<SetSimulationSpeed>()
            .add_message::<SimulationPauseRequest>()
            .add_message::<SimulationControlChanged>()
            .add_message::<ZooDayAdvanced>()
            .add_message::<ZooTickAdvanced>()
            .add_message::<ZooYearAdvanced>()
            .add_message::<ZooSlowTickElapsed>()
            .add_message::<ZooHeartbeatElapsed>()
            .add_systems(
                Update,
                initialize_simulation_time_from_selected_world
                    .in_set(WorldLoadPreparationSet::InitializeClock)
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::Loading)),
            )
            .add_systems(
                Update,
                (
                    route_pause_game_action_to_simulation_control_requests
                        .run_if(on_message::<ActionRequest>),
                    apply_simulation_control_requests
                        .after(route_pause_game_action_to_simulation_control_requests)
                        .after(initialize_simulation_time_from_selected_world)
                        .run_if(
                            on_message::<SetSimulationSpeed>
                                .or_eager(on_message::<SimulationPauseRequest>),
                        ),
                    project_simulation_speed
                        .after(initialize_simulation_time_from_selected_world)
                        .after(apply_simulation_control_requests)
                        .run_if(simulation_speed_inputs_changed),
                )
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                route_authored_ui_simulation_actions_to_simulation_control_requests
                    .in_set(UiSet::Routing),
            )
            .add_systems(
                PostUpdate,
                project_active_simulation_pause_claims
                    .after(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate)
                    .run_if(pause_claim_inputs_changed),
            )
            .add_systems(
                Update,
                (
                    zoo_date_ui_presentation::project_localized_zoo_date
                        .run_if(zoo_date_ui_presentation::zoo_date_inputs_changed),
                    simulation_control_ui_presentation::project_simulation_paused_node_visibility,
                    simulation_control_ui_presentation::project_simulation_pause_toggle_selection,
                )
                    .in_set(UiSet::DomainProjection)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    zoo_clock_advancement::advance_authoritative_zoo_clock_and_calendar,
                    timekeeper_cadence::advance_slow_tick,
                )
                    .in_set(FixedGameSet::Clock)
                    .run_if(in_state(GamePhase::InGame))
                    .run_if(simulation_control_queries::simulation_is_running),
            )
            .add_systems(
                Update,
                timekeeper_cadence::advance_heartbeat
                    .after(initialize_simulation_time_from_selected_world),
            );
    }
}

#[cfg(test)]
mod zoo_calendar_and_clock_tests;
