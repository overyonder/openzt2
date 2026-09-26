mod authored_water_surface_impact_projection;
pub(crate) mod collider_hydration;
pub(crate) mod fitting_surface_operations;
pub(crate) mod fitting_surface_types;
pub(crate) mod interaction_execution;
pub(crate) mod interaction_types;

use authored_water_surface_impact_projection::project_authored_body_water_surface_crossings_into_impact_waves;
use collider_hydration::{
    hydrate_world_boundary_colliders,
    replace_loaded_model_collider_requests_with_avian_triangle_meshes,
};
use fitting_surface_operations::fit_eligible_static_objects_to_changed_terrain_surfaces;
use interaction_execution::{
    apply_requested_impulses_to_dynamic_avian_bodies,
    publish_gameplay_contact_facts_from_avian_collision_events,
    synchronize_derived_moving_components_from_changed_avian_velocities,
};

use interaction_types::{
    ApplyPhysicsImpulseRequest, PhysicsContactFact, PhysicsInteractionFailure,
};

use avian3d::prelude::PhysicsSystems;
use bevy::prelude::*;

use crate::application_lifecycle::GamePhase;

/// Applies game interactions to Avian bodies.
/// Root integration installs and configures `PhysicsPlugins` before this plugin.
pub(super) struct ZooPhysicsPlugin;

impl Plugin for ZooPhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ApplyPhysicsImpulseRequest>()
            .add_message::<PhysicsContactFact>()
            .add_message::<PhysicsInteractionFailure>()
            .configure_sets(
                FixedPostUpdate,
                (
                    PhysicsSystems::First,
                    PhysicsSystems::Prepare,
                    PhysicsSystems::StepSimulation,
                    PhysicsSystems::Writeback,
                    PhysicsSystems::Last,
                )
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedPostUpdate,
                (
                    apply_requested_impulses_to_dynamic_avian_bodies,
                    fit_eligible_static_objects_to_changed_terrain_surfaces,
                    replace_loaded_model_collider_requests_with_avian_triangle_meshes,
                    hydrate_world_boundary_colliders,
                )
                    .before(PhysicsSystems::Prepare),
            )
            .add_systems(
                FixedPostUpdate,
                (
                    publish_gameplay_contact_facts_from_avian_collision_events,
                    synchronize_derived_moving_components_from_changed_avian_velocities,
                    project_authored_body_water_surface_crossings_into_impact_waves,
                )
                    .after(PhysicsSystems::Writeback),
            );
    }
}

#[cfg(test)]
mod tests;
