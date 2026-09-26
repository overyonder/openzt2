mod ambient_animal_lifecycle;
mod ambient_animal_spawn_rules;
mod ambient_animal_types;
mod authored_wind_shader_presentation;
mod daylight_curve_sampling;
mod daylight_environment_presentation;
mod daylight_environment_sampling;
mod earthquake_ui_action_routing;
mod environment_fog_attachment;
mod environment_presentation_hydration;
pub(crate) mod environment_presentation_types;
mod environment_random_sampling;
pub(crate) mod environment_state_types;
mod environment_visual_construction;
mod weather_animal_welfare;
mod weather_rules;
mod weather_transition_lifecycle;
mod weather_types;
mod world_environment_cleanup;
mod world_environment_initialization;

use bevy::prelude::*;

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::{FixedGameSet, GameSet};

use self::weather_types::{
    SetWeather, WeatherChanged, WeatherRequestApplied, WeatherRequestRejected,
};

pub struct EnvironmentPlugin;

impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SetWeather>()
            .add_message::<WeatherRequestApplied>()
            .add_message::<WeatherRequestRejected>()
            .add_message::<WeatherChanged>()
            .add_systems(
                Update,
                world_environment_initialization::initialize_world_environment_from_loaded_scenario_and_definitions
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                earthquake_ui_action_routing::
                    route_authored_earthquake_ui_actions_to_world_environment_state
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                PostUpdate,
                daylight_environment_presentation::center_camera_relative_environment_visuals_on_active_zoo_camera
                    .before(TransformSystems::Propagate)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    authored_wind_shader_presentation::advance_authored_wind_shader_presentation_state,
                    environment_presentation_hydration::hydrate_bevy_renderer_entities_for_new_world_environment,
                    environment_visual_construction::attach_environment_sun_visuals_to_loaded_animation_joints,
                    environment_fog_attachment::attach_neutral_environment_fog_to_new_zoo_cameras,
                    daylight_environment_presentation::project_current_daylight_weather_and_sky_into_bevy_renderer,
                )
                    .chain()
                    .in_set(GameSet::Presentation)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    weather_transition_lifecycle::advance_automatic_and_requested_weather_transitions,
                    weather_animal_welfare::apply_current_weather_welfare_adjustment_to_zoo_animals
                        .after(weather_transition_lifecycle::advance_automatic_and_requested_weather_transitions),
                    ambient_animal_lifecycle::spawn_eligible_ambient_animals_from_authored_environment_spawners,
                )
                    .in_set(FixedGameSet::Think)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                weather_transition_lifecycle::apply_requested_weather_transitions
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                ambient_animal_lifecycle::despawn_ambient_animals_after_authored_lifetime_expires
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                OnExit(GamePhase::InGame),
                world_environment_cleanup::remove_world_environment_entities_and_camera_fog_after_leaving_game,
            );
    }
}

#[cfg(test)]
mod daylight_curve_sampling_tests;
#[cfg(test)]
mod environment_presentation_lifecycle_tests;
