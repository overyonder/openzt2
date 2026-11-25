use openzt2_world_state::simulation_time::{
    deterministic_random_stream, simulation_clock_types, simulation_control_queries,
    simulation_control_types, zoo_calendar_calculation,
};

mod authored_simulation_control_bindings;
mod simulation_control_action_routing;
mod simulation_control_application;
mod simulation_control_ui_presentation;
mod simulation_pause_claim_projection;
mod simulation_speed_override;
pub mod simulation_time_world_lifecycle;
mod timekeeper_cadence;
mod zoo_clock_advancement;
mod zoo_date_ui_presentation;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_interface_state::{
    input::input_types::ActionRequest,
    ui::{authored_ui_activation_contracts::UiNodeActivated, UiSet},
};
use openzt2_world_state::{
    application_schedule::{FixedGameSet, GameSet, ZooHeartbeat, ZooSlowTick},
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
            apply_simulation_control_requests, project_simulation_speed,
            simulation_speed_inputs_changed,
        };
        use simulation_time_world_lifecycle::initialize_simulation_time_from_selected_world;

        simulation_pause_claim_projection::configure_pause_claim_projection(app);
        app.init_resource::<simulation_speed_override::SimulationSpeedOverride>()
            .add_message::<SetSimulationSpeed>()
            .add_message::<SimulationPauseRequest>()
            .add_message::<SimulationControlChanged>()
            .add_message::<ZooDayAdvanced>()
            .add_message::<ZooTickAdvanced>()
            .add_message::<ZooYearAdvanced>()
            .add_message::<ZooSlowTickElapsed>()
            .add_message::<ZooHeartbeatElapsed>()
            .configure_sets(
                FixedUpdate,
                ZooSlowTick
                    .after(timekeeper_cadence::advance_slow_tick)
                    .run_if(on_message::<ZooSlowTickElapsed>),
            )
            .configure_sets(
                RunFixedMainLoop,
                ZooHeartbeat
                    .in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop)
                    .after(timekeeper_cadence::advance_heartbeat)
                    .run_if(on_message::<ZooHeartbeatElapsed>),
            )
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
                    .in_set(UiSet::Routing)
                    .run_if(on_message::<ActionRequest>.or_eager(on_message::<UiNodeActivated>)),
            )
            .add_systems(
                Update,
                (
                    authored_simulation_control_bindings::bind_authored_zoo_date_text,
                    authored_simulation_control_bindings::bind_authored_pause_toggles,
                    zoo_date_ui_presentation::project_localized_zoo_date
                        .after(authored_simulation_control_bindings::bind_authored_zoo_date_text)
                        .run_if(zoo_date_ui_presentation::zoo_date_inputs_changed),
                    simulation_control_ui_presentation::project_simulation_paused_node_visibility
                        .run_if(simulation_control_ui_presentation::pause_indicators_changed),
                    simulation_control_ui_presentation::project_simulation_pause_toggle_selection
                        .after(authored_simulation_control_bindings::bind_authored_pause_toggles)
                        .run_if(simulation_control_ui_presentation::pause_toggles_changed),
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
                RunFixedMainLoop,
                timekeeper_cadence::advance_heartbeat
                    .in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop),
            );
    }
}

#[cfg(test)]
mod zoo_calendar_and_clock_tests;
