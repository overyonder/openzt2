pub(crate) mod authored_water_surface_impact_projection;
mod behavior_fall_execution;
pub mod collider_hydration;
pub(crate) mod fitting_surface_operations;
pub(crate) mod interaction_execution;
pub(crate) mod interaction_types;
pub mod water_surface_disturbance_projection;

use crate::physics::authored_water_surface_impact_projection::bind_authored_water_impact_bodies;
use crate::physics::authored_water_surface_impact_projection::project_authored_body_water_surface_crossings_into_impact_waves;
use crate::physics::collider_hydration::hydrate_world_boundary_colliders;
use crate::physics::collider_hydration::replace_loaded_model_collider_requests_with_avian_triangle_meshes;
use crate::physics::fitting_surface_operations::{
    fit_objects_with_changed_inputs_or_terrain, ChangedTerrainFitting,
};
use crate::physics::interaction_execution::apply_requested_impulses_to_dynamic_avian_bodies;

use crate::physics::interaction_types::ApplyPhysicsImpulseRequest;
use crate::physics::interaction_types::PhysicsInteractionFailure;

use avian3d::prelude::PhysicsSystems;
use bevy::ecs::schedule::SystemCondition;
use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::physics::fitting_surface_types::TerrainFittingSurface;
use openzt2_world_state::simulation_time::simulation_control_queries::simulation_is_running;
use openzt2_zoo_state::{
    locomotion::locomotion_types::NavAgent,
    terrain::{terrain_chunk_types::TerrainIndex, terrain_edit_types::TerrainChanged},
};

/// Applies game interactions to Avian bodies.
/// Root integration installs and configures `PhysicsPlugins` before this plugin.
pub struct ZooPhysicsPlugin;

impl Plugin for ZooPhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(interaction_execution::release_behavior_object_into_world)
            .add_observer(behavior_fall_execution::start_authored_fall)
            .add_message::<ApplyPhysicsImpulseRequest>()
            .add_message::<PhysicsInteractionFailure>()
            .add_systems(
                PostUpdate,
                water_surface_disturbance_projection::project_animation_surface_effects_into_water_disturbances
                    .after(TransformSystems::Propagate),
            )
            .configure_sets(
                FixedPostUpdate,
                (
                    PhysicsSystems::First,
                    PhysicsSystems::Prepare,
                    PhysicsSystems::StepSimulation,
                    PhysicsSystems::Writeback,
                    PhysicsSystems::Last,
                )
                    .run_if(in_state(GamePhase::InGame))
                    .run_if(simulation_is_running),
            )
            .add_systems(
                FixedPostUpdate,
                (
                    fit_objects_with_changed_inputs_or_terrain.run_if(
                        any_with_component::<TerrainIndex>.and_then(
                            any_match_filter::<ChangedTerrainFitting>
                                .or_else(on_message::<TerrainChanged>)
                                .or_else(any_match_filter::<Added<NavAgent>>)
                                .or_else(any_component_removed::<NavAgent>)
                                .or_else(any_component_removed::<TerrainFittingSurface>)
                                .or_else(any_match_filter::<Changed<TerrainIndex>>),
                        ),
                    ),
                    replace_loaded_model_collider_requests_with_avian_triangle_meshes,
                    hydrate_world_boundary_colliders,
                )
                    .before(PhysicsSystems::Prepare)
                    .run_if(in_state(GamePhase::Loading).or_else(in_state(GamePhase::InGame))),
            )
            .add_systems(
                FixedPostUpdate,
                (
                    apply_requested_impulses_to_dynamic_avian_bodies,
                    interaction_execution::advance_falling_released_objects
                        .after(fit_objects_with_changed_inputs_or_terrain),
                )
                    .before(PhysicsSystems::Prepare)
                    .run_if(in_state(GamePhase::InGame))
                    .run_if(simulation_is_running),
            )
            .add_systems(
                FixedPostUpdate,
                project_authored_body_water_surface_crossings_into_impact_waves
                    .after(PhysicsSystems::Writeback)
                    .after(bind_authored_water_impact_bodies)
                    .run_if(in_state(GamePhase::InGame))
                    .run_if(simulation_is_running),
            )
            .add_systems(
                FixedPostUpdate,
                bind_authored_water_impact_bodies
                    .after(PhysicsSystems::Prepare)
                    .run_if(in_state(GamePhase::InGame))
                    .run_if(simulation_is_running),
            );
    }
}

#[cfg(test)]
mod tests;
