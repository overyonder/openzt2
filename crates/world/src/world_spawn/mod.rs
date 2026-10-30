use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadPreparationSet;
pub mod prefab_renderable_components;
pub mod prefab_world_instance_spawning;
// One-time translation from authored scenario and prefab rows into the live ECS.

pub mod expanding_column_presentation;
mod named_physical_presentation_projection;
mod persistent_id_assignment;
mod physical_presentation_state;
mod prefab_attachment_independent_rotation;
mod prefab_authored_rotation_cycle_advancement;
mod prefab_authored_transform_animation_advancement;
mod prefab_object_presentation_attachment_projection;
pub mod prefab_presentation_render_tree;

#[cfg(test)]
mod persistent_id_tests;
#[cfg(test)]
mod world_unload_tests;

mod starting_path_hydration;
mod starting_topology_hydration;
pub mod world_hydration_completion;
pub mod world_load_request_acceptance;
pub mod world_persistent_id_reservation;
mod world_prefab_record_hydration;
mod world_spawn_value_application;
mod world_terrain_hydration;
pub mod world_unloading;
mod zoo_entrance_anchor_synchronization;

use crate::world_spawn::persistent_id_assignment::assign_requested_persistent_ids;
use crate::world_spawn::prefab_authored_rotation_cycle_advancement::advance_authored_prefab_rotation_cycles_from_elapsed_real_time;
use crate::world_spawn::prefab_authored_transform_animation_advancement::advance_authored_prefab_transform_animations_from_elapsed_time;
use crate::world_spawn::prefab_object_presentation_attachment_projection::project_default_authored_world_object_presentation_attachments;
use crate::world_spawn::prefab_presentation_render_tree::hydrate_prefab_presentations;
use crate::world_spawn::starting_path_hydration::hydrate_all_starting_paths_into_topology_once;
use crate::world_spawn::starting_topology_hydration::hydrate_all_starting_fence_nodes_and_edges_into_topology_once;
use crate::world_spawn::world_hydration_completion::complete_or_fail_finished_world_hydration;
use crate::world_spawn::world_load_request_acceptance::accept_queued_world_load_after_required_assets_resolve;
use crate::world_spawn::world_persistent_id_reservation::reserve_all_imported_world_persistent_ids;
use crate::world_spawn::world_prefab_record_hydration::hydrate_bounded_world_prefab_record_batch;
use crate::world_spawn::world_spawn_value_application::apply_all_authored_world_spawn_values_once;
use crate::world_spawn::world_terrain_hydration::hydrate_selected_terrain_chunks_world_bounds_and_topology_grid;
use crate::world_spawn::world_unloading::despawn_all_world_roots_and_members_when_leaving_ingame;
use crate::world_spawn::world_unloading::unload_requested_worlds_and_their_members;
use crate::world_spawn::zoo_entrance_anchor_synchronization::synchronize_zoo_entrance_anchor_after_transform_changes;

pub mod prefab_effect_trigger_hydration;
mod world_entity_crating;
mod world_object_selection;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;

pub struct WorldSpawnPlugin;

impl Plugin for WorldSpawnPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream>();
        app.configure_sets(
            Update,
            (
                WorldLoadPreparationSet::Accept,
                WorldLoadPreparationSet::InitializeClock,
                WorldLoadPreparationSet::HydrateTerrain,
            )
                .chain(),
        );
        app.configure_sets(
            Update,
            WorldLoadPreparationSet::InitializeClock
                .run_if(world_hydration_completion::loading_screen_is_ready),
        );
        app.add_message::<openzt2_world_state::world_spawn::physical_presentation_state::PhysicalPresentationRequest>();
        app.init_resource::<openzt2_world_state::world_spawn::world_loading_performance_attribution::WorldLoadingPerformanceAttribution>()
            .add_message::<openzt2_world_state::world_spawn::persistent_id_assignment::AssignPersistentId>()
            .add_message::<openzt2_world_state::world_spawn::persistent_id_assignment::PersistentIdAssigned>()
            .add_message::<openzt2_world_state::world_spawn::persistent_id_assignment::PersistentIdAssignmentFailed>()
            .add_message::<openzt2_world_state::world_spawn::world_load_request_acceptance::BeginWorldLoad>()
            .add_message::<openzt2_world_state::world_spawn::world_hydration_completion::WorldLoadFinished>()
            .add_message::<openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailed>()
            .add_message::<openzt2_world_state::world_spawn::world_unloading::UnloadWorld>()
            .add_message::<openzt2_world_state::world_spawn::world_entity_crating::RemoveWorldEntityFromCrate>()
            .add_systems(
                Update,
                (
                    accept_queued_world_load_after_required_assets_resolve.in_set(WorldLoadPreparationSet::Accept),
                    (
                    hydrate_selected_terrain_chunks_world_bounds_and_topology_grid.in_set(WorldLoadPreparationSet::HydrateTerrain),
                    reserve_all_imported_world_persistent_ids,
                    hydrate_all_starting_fence_nodes_and_edges_into_topology_once,
                    hydrate_all_starting_paths_into_topology_once,
                    hydrate_bounded_world_prefab_record_batch,
                    apply_all_authored_world_spawn_values_once,
                    complete_or_fail_finished_world_hydration
                        .in_set(openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompletionSet),
                    ).chain().run_if(world_hydration_completion::loading_screen_is_ready),
                )
                    .chain()
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::Loading)),
            )
            .add_systems(
                Update,
                (
                    unload_requested_worlds_and_their_members
                        .before(accept_queued_world_load_after_required_assets_resolve),
                    crate::world_spawn::world_entity_crating::remove_requested_world_entities_from_crates,
                    crate::world_spawn::world_object_selection::request_selection_of_clicked_world_object_or_inspectable_ancestor,
                )
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                (
                    advance_authored_prefab_rotation_cycles_from_elapsed_real_time,
                    advance_authored_prefab_transform_animations_from_elapsed_time,
                    crate::world_spawn::world_entity_crating::synchronize_contained_animal_transforms,
                )
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                crate::world_spawn::named_physical_presentation_projection::project_named_presentation_animation_owners
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                (
                    hydrate_prefab_presentations,
                    crate::world_spawn::expanding_column_presentation::align_expanding_column_piece_attachments,
                    crate::world_spawn::physical_presentation_state::apply_physical_presentation_requests,
                    project_default_authored_world_object_presentation_attachments,
                    crate::world_spawn::named_physical_presentation_projection::hydrate_required_named_physical_presentations,
                    crate::world_spawn::named_physical_presentation_projection::attach_named_presentations_to_model_nodes,
                    crate::world_spawn::named_physical_presentation_projection::select_named_presentation_fade_states,
                    crate::world_spawn::named_physical_presentation_projection::project_physical_presentation_caustic_policies,
                    crate::world_spawn::prefab_effect_trigger_hydration::hydrate_prefab_effect_triggers,
                    synchronize_zoo_entrance_anchor_after_transform_changes,
                )
                    .chain()
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                PostUpdate,
                crate::world_spawn::prefab_attachment_independent_rotation::preserve_independent_attachment_orientation
                    .after(bevy::app::AnimationSystems)
                    .before(bevy::transform::TransformSystems::Propagate),
            )
            .add_systems(
                OnExit(GamePhase::InGame),
                despawn_all_world_roots_and_members_when_leaving_ingame
                    .in_set(openzt2_world_state::world_spawn::world_unloading::WorldUnloadSet),
            )
            .add_systems(
                FixedUpdate,
                crate::world_spawn::physical_presentation_state::child_lifecycle::advance_authored_child_lifetimes
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
