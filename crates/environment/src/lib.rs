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
mod environment_presentation_types;
mod environment_random_sampling;
mod environment_state_types;
mod environment_visual_construction;
mod sky_vertex_color_interpolation;
mod underwater_environment_presentation;
mod weather_animal_welfare;
mod weather_rules;
mod weather_transition_lifecycle;
mod weather_types;
mod world_environment_cleanup;
mod world_environment_initialization;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::environment::environment_state_types::ZooTickAdvanced;

use self::weather_types::{
    SetWeather, WeatherChanged, WeatherRequestApplied, WeatherRequestRejected, WeatherWelfareDue,
};

pub struct EnvironmentPlugin;

impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<openzt2_zoo_state::environment::environment_presentation_types::EnvironmentTerrainAmbientLight>()
            .init_resource::<openzt2_zoo_state::environment::environment_state_types::WaterEnvironmentMapSample>()
            .init_resource::<environment_state_types::EnvironmentDaylightFraction>()
            .add_message::<SetWeather>()
            .add_message::<WeatherRequestApplied>()
            .add_message::<WeatherRequestRejected>()
            .add_message::<WeatherChanged>()
            .add_message::<WeatherWelfareDue>()
            .add_message::<ZooTickAdvanced>()
            .add_observer(environment_fog_attachment::remove_camera_fog_when_environment_is_unlinked)
            .add_message::<sky_vertex_color_interpolation::SkyPrimitiveTreeChanged>()
            .add_observer(sky_vertex_color_interpolation::unlink_removed_sky_vertex_color_mesh)
            .add_observer(sky_vertex_color_interpolation::invalidate_discarded_sky_tree_parent)
            .add_systems(
                Update,
                world_environment_initialization::initialize_world_environment_from_loaded_scenario_and_definitions
                    .after(openzt2_interface_state::photos::photo_album_types::DefaultPhotoStorageInitializationSet)
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
                (
                    sky_vertex_color_interpolation::retain_sky_vertex_color_keyframe_models,
                    sky_vertex_color_interpolation::link_first_sky_vertex_color_primitive,
                    sky_vertex_color_interpolation::resolve_sky_vertex_color_meshes,
                    sky_vertex_color_interpolation::interpolate_sky_model_vertex_colors,
                )
                    .chain()
                    .before(TransformSystems::Propagate)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                PostUpdate,
                daylight_environment_presentation::center_camera_relative_environment_visuals_on_active_zoo_camera
                    .before(openzt2_world_state::world_spawn::prefab_authored_billboard_orientation_mode::PrefabBillboardProjection)
                    .before(TransformSystems::Propagate)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    daylight_environment_presentation::sample_environment_daylight_fraction,
                    daylight_environment_presentation::sample_authored_object_ambient_light
                        .after(daylight_environment_presentation::sample_authored_environment_visual_curves),
                    daylight_environment_presentation::sample_authored_environment_visual_curves
                        .after(daylight_environment_presentation::sample_environment_daylight_fraction),
                    authored_wind_shader_presentation::advance_authored_wind_shader_presentation_state,
                    environment_presentation_hydration::hydrate_bevy_renderer_entities_for_new_world_environment,
                    environment_visual_construction::attach_environment_sun_visuals_to_loaded_animation_joints
                        .after(environment_presentation_hydration::hydrate_bevy_renderer_entities_for_new_world_environment),
                    environment_fog_attachment::attach_neutral_environment_fog_to_new_zoo_cameras,
                    daylight_environment_presentation::project_current_daylight_weather_and_sky_into_bevy_renderer
                        .after(daylight_environment_presentation::sample_authored_object_ambient_light)
                        .after(daylight_environment_presentation::sample_authored_environment_visual_curves)
                        .after(environment_visual_construction::attach_environment_sun_visuals_to_loaded_animation_joints),
                    underwater_environment_presentation::update_region_water_light_samples
                        .after(daylight_environment_presentation::sample_authored_environment_visual_curves),
                    underwater_environment_presentation::current::update_water_region_currents
                        .after(authored_wind_shader_presentation::advance_authored_wind_shader_presentation_state),
                    underwater_environment_presentation::project_resolved_camera_distance_fog
                        .after(daylight_environment_presentation::project_current_daylight_weather_and_sky_into_bevy_renderer)
                        .after(environment_fog_attachment::attach_neutral_environment_fog_to_new_zoo_cameras),
                    underwater_environment_presentation::update_water_ray_environment_light_samples
                        .after(daylight_environment_presentation::project_current_daylight_weather_and_sky_into_bevy_renderer),
                    underwater_environment_presentation::sample_water_environment_map
                        .after(daylight_environment_presentation::sample_environment_daylight_fraction),
                )
                    .in_set(openzt2_world_state::application_schedule::WaterPresentationSet::Environment)
                    .in_set(GameSet::Presentation)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    weather_transition_lifecycle::advance_automatic_and_requested_weather_transitions
                        .after(FixedGameSet::Clock)
                        .run_if(on_message::<ZooTickAdvanced>),
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
                world_environment_cleanup::retire_remaining_world_environments_after_leaving_game
                    .after(openzt2_world_state::world_spawn::world_unloading::WorldUnloadSet),
            );
    }
}

#[cfg(test)]
mod daylight_curve_sampling_tests;
#[cfg(test)]
mod environment_presentation_lifecycle_tests;
