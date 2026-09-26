//! One-time translation from authored scenario and prefab rows into the live ECS.

pub(crate) mod expanding_column_presentation;
mod named_physical_presentation_projection;
pub(crate) mod persistent_id_assignment;
pub(crate) mod persistent_id_types;
pub(crate) mod physical_presentation_state;
pub(crate) mod prefab_ambient_light_contribution;
mod prefab_attachment_independent_rotation;
pub(crate) mod prefab_authored_attachment_identifier;
pub(crate) mod prefab_authored_billboard_orientation_mode;
mod prefab_authored_rotation_cycle_advancement;
mod prefab_authored_transform_animation_advancement;
pub(crate) mod prefab_fixed_function_world_lighting_policy;
pub(crate) mod prefab_model_level_of_detail_visibility_range;
pub(crate) mod prefab_model_readiness;
pub(crate) mod prefab_model_tint;
pub(crate) mod prefab_object_presentation_attachment_projection;
pub(crate) mod prefab_presentation_render_tree;
pub(crate) mod prefab_presentation_types;
mod prefab_renderable_components;
pub(crate) mod prefab_source_asset_handle;
pub(super) mod prefab_transform_conversion;
pub(crate) mod prefab_world_instance_spawning;
pub(crate) mod selected_world_identity;
pub(crate) mod selected_world_terrain_asset_handle;
pub(crate) mod world_membership_types;

#[cfg(test)]
mod persistent_id_tests;
#[cfg(test)]
mod world_unload_tests;

mod starting_path_hydration;
mod starting_topology_hydration;
pub(crate) mod world_hydration_completion;
pub(crate) mod world_hydration_types;
pub(crate) mod world_load_completion_marker;
pub(crate) mod world_load_failure;
pub(crate) mod world_load_request_acceptance;
mod world_loading_performance_attribution;
mod world_persistent_id_reservation;
mod world_prefab_record_hydration;
mod world_spawn_value_application;
pub(crate) mod world_terrain_hydration;
pub(crate) mod world_unloading;
pub(crate) mod zoo_entrance_anchor_synchronization;

use persistent_id_assignment::assign_requested_persistent_ids;
use prefab_authored_rotation_cycle_advancement::advance_authored_prefab_rotation_cycles_from_elapsed_real_time;
use prefab_authored_transform_animation_advancement::advance_authored_prefab_transform_animations_from_elapsed_time;
use prefab_object_presentation_attachment_projection::project_default_authored_world_object_presentation_attachments;
use prefab_presentation_render_tree::hydrate_prefab_presentations;
use starting_path_hydration::hydrate_all_starting_paths_into_topology_once;
use starting_topology_hydration::hydrate_all_starting_fence_nodes_and_edges_into_topology_once;
use world_hydration_completion::complete_or_fail_finished_world_hydration;
use world_load_request_acceptance::accept_queued_world_load_after_required_assets_resolve;
use world_persistent_id_reservation::reserve_all_imported_world_persistent_ids;
use world_prefab_record_hydration::hydrate_bounded_world_prefab_record_batch;
use world_spawn_value_application::apply_all_authored_world_spawn_values_once;
use world_terrain_hydration::hydrate_selected_terrain_chunks_world_bounds_and_topology_grid;
use world_unloading::{
    despawn_all_world_roots_and_members_when_leaving_ingame,
    unload_requested_worlds_and_their_members,
};
use zoo_entrance_anchor_synchronization::synchronize_zoo_entrance_anchor_after_transform_changes;

mod prefab_effect_trigger_hydration;
pub(crate) mod world_entity_crating;
mod world_object_selection;

use bevy::prelude::*;

use crate::{
    application_lifecycle::GamePhase,
    application_schedule::{FixedGameSet, GameSet},
};

pub(super) struct WorldSpawnPlugin;

impl Plugin for WorldSpawnPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<physical_presentation_state::PhysicalPresentationRequest>();
        app.init_resource::<world_loading_performance_attribution::WorldLoadingPerformanceAttribution>()
            .add_message::<persistent_id_assignment::AssignPersistentId>()
            .add_message::<persistent_id_assignment::PersistentIdAssigned>()
            .add_message::<persistent_id_assignment::PersistentIdAssignmentFailed>()
            .add_message::<world_load_request_acceptance::BeginWorldLoad>()
            .add_message::<world_hydration_completion::WorldLoadFinished>()
            .add_message::<world_load_failure::WorldLoadFailed>()
            .add_message::<world_unloading::UnloadWorld>()
            .add_message::<world_entity_crating::RemoveWorldEntityFromCrate>()
            .add_systems(
                Update,
                (
                    accept_queued_world_load_after_required_assets_resolve,
                    crate::plugins::simulation_time::simulation_time_world_lifecycle::initialize_simulation_time_from_selected_world,
                    hydrate_selected_terrain_chunks_world_bounds_and_topology_grid,
                    reserve_all_imported_world_persistent_ids,
                    hydrate_all_starting_fence_nodes_and_edges_into_topology_once,
                    hydrate_all_starting_paths_into_topology_once,
                    hydrate_bounded_world_prefab_record_batch,
                    apply_all_authored_world_spawn_values_once,
                    complete_or_fail_finished_world_hydration,
                )
                    .chain()
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::Loading)),
            )
            .add_systems(
                Update,
                (
                    unload_requested_worlds_and_their_members,
                    world_entity_crating::remove_requested_world_entities_from_crates,
                    world_object_selection::request_selection_of_clicked_world_object_or_inspectable_ancestor,
                )
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                (
                    advance_authored_prefab_rotation_cycles_from_elapsed_real_time,
                    advance_authored_prefab_transform_animations_from_elapsed_time,
                )
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                (
                    hydrate_prefab_presentations,
                    expanding_column_presentation::align_expanding_column_piece_attachments,
                    physical_presentation_state::apply_physical_presentation_requests,
                    project_default_authored_world_object_presentation_attachments,
                    named_physical_presentation_projection::hydrate_required_named_physical_presentations,
                    prefab_effect_trigger_hydration::hydrate_prefab_effect_triggers,
                    synchronize_zoo_entrance_anchor_after_transform_changes,
                )
                    .chain()
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                PostUpdate,
                prefab_attachment_independent_rotation::preserve_independent_attachment_orientation
                    .after(bevy::app::AnimationSystems)
                    .before(bevy::transform::TransformSystems::Propagate),
            )
            .add_systems(
                OnExit(GamePhase::InGame),
                despawn_all_world_roots_and_members_when_leaving_ingame,
            )
            .add_systems(
                FixedUpdate,
                physical_presentation_state::child_lifecycle::advance_authored_child_lifetimes
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                assign_requested_persistent_ids
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}
