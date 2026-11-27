mod ambient_animal_lifecycle;
mod ambient_animal_types;
mod authored_wind_shader_presentation;
mod daylight_curve_sampling;
mod daylight_environment_presentation;
mod daylight_environment_sampling;
mod environment_fog_attachment;
mod environment_map_sampling;
mod environment_presentation_hydration;
mod environment_presentation_types;
mod environment_random_sampling;
mod environment_state_types;
mod environment_visual_construction;
mod scripted_overhead_camera_motion;
mod sky_vertex_color_interpolation;
mod underwater_environment_presentation;
mod weather_animal_welfare;
mod weather_rules;
mod weather_transition_lifecycle;
mod weather_types;
mod world_environment_initialization;

pub use world_environment_initialization::AdditionalAmbientActors;

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
        app.add_message::<SetWeather>()
            .add_message::<openzt2_interface_state::information::award_presentation_types::AwardProgramEffect>()
            .register_required_components::<openzt2_world_state::world_spawn::world_membership_types::WorldRoot, AdditionalAmbientActors>()
            .add_systems(
                Update,
                world_environment_initialization::register_award_ambient_actors
                    .run_if(on_message::<openzt2_interface_state::information::award_presentation_types::AwardProgramEffect>),
            )
            .register_required_components::<ambient_animal_types::AmbientSpawnCadence, ambient_animal_lifecycle::WorldAmbientCandidates>()
            .add_message::<openzt2_interface_state::camera::camera_control_message_types::ScriptedOverheadCameraMotion>()
            .add_message::<WeatherRequestApplied>()
            .add_message::<WeatherRequestRejected>()
            .add_message::<WeatherChanged>()
            .add_message::<WeatherWelfareDue>()
            .add_message::<ZooTickAdvanced>()
            .add_observer(environment_fog_attachment::remove_camera_fog_when_environment_is_unlinked)
            .add_observer(ambient_animal_lifecycle::retire_effects_attached_to_removed_ambient_animal)
            .add_message::<sky_vertex_color_interpolation::SkyPrimitiveTreeChanged>()
            .add_observer(sky_vertex_color_interpolation::unlink_removed_sky_vertex_color_mesh)
            .add_observer(sky_vertex_color_interpolation::invalidate_discarded_sky_tree_parent)
            .add_observer(sky_vertex_color_interpolation::restore_unlinked_sky_base_mesh)
            .add_observer(environment_visual_construction::attach_sun_visual_when_position_model_is_ready)
            .add_observer(environment_visual_construction::attach_sun_visual_to_single_node_position_model)
            .add_systems(
                Update,
                world_environment_initialization::initialize_world_environment_from_loaded_scenario_and_definitions
                    .after(openzt2_interface_state::photos::photo_album_types::DefaultPhotoStorageInitializationSet)
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                PostUpdate,
                (
                    sky_vertex_color_interpolation::retain_sky_vertex_color_keyframe_models,
                    sky_vertex_color_interpolation::link_first_sky_vertex_color_primitive,
                    sky_vertex_color_interpolation::resolve_sky_vertex_color_meshes
                        .after(sky_vertex_color_interpolation::retain_sky_vertex_color_keyframe_models),
                    sky_vertex_color_interpolation::project_changed_sky_vertex_colors
                        .after(sky_vertex_color_interpolation::resolve_sky_vertex_color_meshes)
                        .after(sky_vertex_color_interpolation::link_first_sky_vertex_color_primitive),
                )
                    .before(TransformSystems::Propagate)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                scripted_overhead_camera_motion::apply_scripted_overhead_camera_motion
                    .after(openzt2_interface_state::ui::UiSet::DomainProjection)
                    .in_set(GameSet::Ui)
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
                    daylight_environment_presentation::sample_authored_object_ambient_light
                        .after(daylight_environment_presentation::sample_authored_environment_visual_curves),
                    daylight_environment_presentation::sample_authored_environment_visual_curves,
                    authored_wind_shader_presentation::advance_authored_wind_shader_presentation_state,
                    environment_presentation_hydration::hydrate_bevy_renderer_entities_for_new_world_environment,
                    environment_fog_attachment::attach_neutral_environment_fog_to_new_zoo_cameras,
                    daylight_environment_presentation::project_current_daylight_weather_and_sky_into_bevy_renderer
                        .after(daylight_environment_presentation::sample_authored_object_ambient_light)
                        .after(environment_presentation_hydration::hydrate_bevy_renderer_entities_for_new_world_environment),
                    underwater_environment_presentation::update_region_water_light_samples
                        .after(daylight_environment_presentation::sample_authored_environment_visual_curves),
                    underwater_environment_presentation::current::update_water_region_currents
                        .after(authored_wind_shader_presentation::advance_authored_wind_shader_presentation_state),
                    underwater_environment_presentation::project_resolved_camera_distance_fog
                        .after(daylight_environment_presentation::project_current_daylight_weather_and_sky_into_bevy_renderer)
                        .after(environment_fog_attachment::attach_neutral_environment_fog_to_new_zoo_cameras),
                    underwater_environment_presentation::update_water_ray_environment_light_samples
                        .after(daylight_environment_presentation::project_current_daylight_weather_and_sky_into_bevy_renderer),
                    environment_map_sampling::sample_changed_environment_maps,
                )
                    .in_set(openzt2_world_state::application_schedule::WaterPresentationSet::Environment)
                    .in_set(GameSet::Presentation)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    daylight_environment_presentation::publish_world_daylight_from_zoo_clock
                        .after(FixedGameSet::Clock),
                    weather_transition_lifecycle::advance_automatic_and_requested_weather_transitions
                        .after(daylight_environment_presentation::publish_world_daylight_from_zoo_clock)
                        .run_if(on_message::<ZooTickAdvanced>),
                    weather_animal_welfare::apply_current_weather_welfare_adjustment_to_zoo_animals
                        .after(weather_transition_lifecycle::advance_automatic_and_requested_weather_transitions),
                    ambient_animal_lifecycle::attempt_world_ambient_spawn
                        .after(daylight_environment_presentation::publish_world_daylight_from_zoo_clock)
                        .after(ambient_animal_lifecycle::reconcile_world_ambient_candidates),
                    ambient_animal_lifecycle::reconcile_world_ambient_candidates,
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
                ambient_animal_lifecycle::retire_expired_world_ambient_animals
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}

#[cfg(test)]
mod daylight_curve_sampling_tests;
#[cfg(test)]
mod environment_presentation_lifecycle_tests;
