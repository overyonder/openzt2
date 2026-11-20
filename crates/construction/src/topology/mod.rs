//! Fence, path, gate, and portal facts stored directly in the live ECS.

mod fence_anchor_cursor_presentation;
pub mod fence_anchor_press_diagnostic_types;
mod fence_construction_preview_presentation;
mod fence_segment_prefab_refresh;
pub mod fence_terrain_endpoint_skew_presentation;
mod gate_automatic_operation;
mod gate_state_application;
mod loaded_topology_validation;
pub mod path_curb_presentation;
mod path_placement_highlight_presentation;
mod path_presentation_neighborhood;
mod path_support_hydration;
mod path_surface_presentation;
mod tank_wall_rail_presentation;
pub mod topology_construction_interaction;
mod topology_construction_planning;
pub mod topology_construction_preview_types;
mod topology_construction_transaction_preparation;
mod topology_definition_hydration;
mod topology_deletion_transaction_preparation;
mod topology_edit_application;
mod topology_edit_outcome_messages;
mod topology_prefab_presentation;
pub mod topology_presentation_types;

use crate::topology::fence_anchor_cursor_presentation::present_fence_anchor_cursor;
use crate::topology::fence_construction_preview_presentation::project_fence_construction_preview_validity_onto_prefab_model_tints;
use crate::topology::fence_construction_preview_presentation::reconcile_fence_construction_preview_presentation_segments;
use crate::topology::fence_segment_prefab_refresh::refresh_fence_segment_prefabs_after_topology_index_changes;
use crate::topology::fence_terrain_endpoint_skew_presentation::fit_fence_presentations_to_terrain_endpoint_heights;
use crate::topology::gate_automatic_operation::request_open_gates_for_agents_within_authored_trigger_distance;
use crate::topology::gate_automatic_operation::tick_armed_gate_automatic_close_timers;
use crate::topology::gate_state_application::apply_requested_gate_state;
use crate::topology::gate_state_application::attach_authored_animation_sets_to_resolved_gate_models;
use crate::topology::gate_state_application::project_gate_state_to_ready_animation_presentations;
use crate::topology::loaded_topology_validation::remove_duplicate_or_invalid_loaded_topology;
use crate::topology::path_curb_presentation::hydrate_authored_path_curb_prefab_presentations;
use crate::topology::path_curb_presentation::invalidate_path_curb_presentations_after_topology_or_terrain_changes;
use crate::topology::path_support_hydration::hydrate_authored_path_supports_at_sampled_terrain_height;
use crate::topology::path_support_hydration::invalidate_path_supports_after_topology_or_terrain_changes;
use crate::topology::path_surface_presentation::fit_ground_path_tiles_to_authored_terrain;
use crate::topology::path_surface_presentation::hydrate_elevated_path_surface_mesh_and_material_presentations;
use crate::topology::path_surface_presentation::invalidate_ground_path_terrain_fitting_after_terrain_changes;
use crate::topology::path_surface_presentation::ElevatedPathSurfacePresentationAssets;
use crate::topology::topology_construction_interaction::apply_fence_placement_requests_to_construction_previews;
use crate::topology::topology_construction_interaction::apply_path_placement_requests_to_construction_previews;
use crate::topology::topology_construction_interaction::drive_selected_topology_placement_pointer_sequence;
use crate::topology::topology_construction_interaction::evaluate_topology_construction_preview_validity;
use crate::topology::topology_construction_interaction::restart_fence_preview_from_committed_endpoint;
use crate::topology::topology_construction_interaction::restart_path_preview_from_committed_endpoint;
use crate::topology::topology_construction_preview_types::PlaceFence;
use crate::topology::topology_construction_preview_types::PlacePath;
use crate::topology::topology_construction_transaction_preparation::prepare_topology_construction_transaction;
use crate::topology::topology_definition_hydration::hydrate_save_loaded_topology_definition_facts;
use crate::topology::topology_deletion_transaction_preparation::prepare_topology_deletion_transaction;
use crate::topology::topology_edit_application::apply_committed_topology_edit;
use crate::topology::topology_prefab_presentation::attach_fence_posts_to_hydrated_segment_models;
use crate::topology::topology_prefab_presentation::hydrate_topology_prefab_presentations;
use openzt2_zoo_state::topology::gate_operation_types::GateOperationSet;
use openzt2_zoo_state::topology::gate_operation_types::SetGateState;
use openzt2_zoo_state::topology::topology_edit_types::CommitTopologyEdit;
use openzt2_zoo_state::topology::topology_edit_types::TopologyChanged;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditAcknowledged;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPreparationRejected;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPrepared;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::construction::FixedConstructionPreparedDomainEditMaterializationSet;
use openzt2_zoo_state::construction::FixedConstructionSet;

/// Registers topology-owned messages and the bounded spatial lookup. Root integration
/// places the exported systems in the shared construction schedules.
pub struct TopologyPlugin;

impl Plugin for TopologyPlugin {
    fn build(&self, app: &mut App) {
        use openzt2_zoo_state::topology::persistent_edit_lookup::PersistentEditLookupPlugin;
        if !app.is_plugin_added::<PersistentEditLookupPlugin>() {
            app.add_plugins(PersistentEditLookupPlugin);
        }
        app.init_resource::<TopologyIndex>()
            .init_resource::<TopologyGrid>()
            .add_observer(detach_removed_fence_endpoints)
            .add_observer(despawn_fence_with_removed_endpoint)
            .add_observer(path_support_hydration::remove_supports_when_path_tile_is_removed)
            .add_observer(fence_construction_preview_presentation::remove_fence_segments_on_preview_removal)
            .add_observer(tank_wall_rail_presentation::remove_tank_wall_rails_on_component_removal)
            .add_observer(topology_construction_interaction::drop_prompt_candidates_when_fence_component_is_removed)
            .init_resource::<ElevatedPathSurfacePresentationAssets>()
            .init_resource::<path_placement_highlight_presentation::PathPlacementHighlightAssets>()
            .add_message::<PlaceFence>()
            .add_message::<PlacePath>()
            .add_message::<SetGateState>()
            .add_message::<TopologyChanged>()
            .add_message::<openzt2_zoo_state::topology::topology_edit_types::FenceConnectivityChanged>()
            .add_message::<openzt2_zoo_state::topology::topology_edit_types::PathConnectivityChanged>()
            .add_message::<TopologyEditPrepared>()
            .add_message::<TopologyEditPreparationRejected>()
            .add_message::<CommitTopologyEdit>()
            .add_message::<TopologyEditAcknowledged>()
            .add_systems(
                OnEnter(GamePhase::InGame),
                remove_duplicate_or_invalid_loaded_topology,
            )
            .add_systems(
                Update,
                (
                    topology_construction_interaction::track_rectangular_enclosure_gate_prompt,
                    restart_fence_preview_from_committed_endpoint,
                    restart_path_preview_from_committed_endpoint,
                    topology_construction_interaction::rotate_closed_fence_placement_heading,
                    drive_selected_topology_placement_pointer_sequence,
                    apply_fence_placement_requests_to_construction_previews,
                    apply_path_placement_requests_to_construction_previews,
                    evaluate_topology_construction_preview_validity,
                )
                    .chain()
                    .after(openzt2_zoo_state::construction::ConstructionCursorUpdateSet)
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                (
                    hydrate_save_loaded_topology_definition_facts,
                    reconcile_fence_construction_preview_presentation_segments,
                    present_fence_anchor_cursor,
                    invalidate_ground_path_terrain_fitting_after_terrain_changes,
                    fit_ground_path_tiles_to_authored_terrain
                        .after(invalidate_ground_path_terrain_fitting_after_terrain_changes),
                    hydrate_elevated_path_surface_mesh_and_material_presentations,
                    invalidate_path_curb_presentations_after_topology_or_terrain_changes,
                    hydrate_authored_path_curb_prefab_presentations
                        .after(invalidate_path_curb_presentations_after_topology_or_terrain_changes)
                        .after(fit_ground_path_tiles_to_authored_terrain),
                    refresh_fence_segment_prefabs_after_topology_index_changes
                        .after(hydrate_save_loaded_topology_definition_facts),
                    hydrate_topology_prefab_presentations
                        .after(reconcile_fence_construction_preview_presentation_segments)
                        .after(refresh_fence_segment_prefabs_after_topology_index_changes)
                        .after(hydrate_save_loaded_topology_definition_facts),
                    fence_construction_preview_presentation::synchronize_fence_preview_visibility_with_pointer_capture
                        .after(reconcile_fence_construction_preview_presentation_segments),
                    attach_fence_posts_to_hydrated_segment_models.after(hydrate_topology_prefab_presentations),
                    attach_authored_animation_sets_to_resolved_gate_models.after(hydrate_topology_prefab_presentations),
                    fit_fence_presentations_to_terrain_endpoint_heights.after(hydrate_topology_prefab_presentations),
                    project_fence_construction_preview_validity_onto_prefab_model_tints
                        .after(hydrate_topology_prefab_presentations),
                    tank_wall_rail_presentation::present_changed_tank_wall_rails
                        .after(fit_fence_presentations_to_terrain_endpoint_heights),
                )
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                path_placement_highlight_presentation::present_ground_path_placement_highlights
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                PostUpdate,
                (
                    crate::construction::capture_confirmed_preview::<topology_construction_preview_types::FenceDrawPlan>,
                    crate::construction::capture_confirmed_preview::<topology_construction_preview_types::PathDrawPlan>,
                ),
            )
            .add_systems(
                FixedUpdate,
                (
                    crate::construction::clear_confirmed_previews::<topology_construction_preview_types::FenceDrawPlan>,
                    crate::construction::clear_confirmed_previews::<topology_construction_preview_types::PathDrawPlan>,
                )
                    .after(FixedConstructionSet::Prepare)
                    .before(FixedConstructionSet::Apply),
            )
            .add_systems(
                FixedUpdate,
                (
                    prepare_topology_construction_transaction
                        .after(
                            openzt2_zoo_state::construction::FixedConstructionTransactionMaterializationSet,
                        )
                        .before(FixedConstructionPreparedDomainEditMaterializationSet),
                    prepare_topology_deletion_transaction
                        .after(
                            openzt2_zoo_state::construction::FixedConstructionTransactionMaterializationSet,
                        )
                        .before(FixedConstructionPreparedDomainEditMaterializationSet),
                )
                    .in_set(FixedConstructionSet::Prepare),
            )
            .add_systems(
                FixedUpdate,
                apply_committed_topology_edit
                    .after(openzt2_zoo_state::construction::ConstructionDomainApplicationDispatchSet)
                    .after(openzt2_zoo_state::construction::ConstructionFailureRollbackDispatchSet)
                    .before(openzt2_zoo_state::construction::ConstructionDomainAcknowledgementCollectionSet)
                    .in_set(FixedConstructionSet::Apply),
            )
            .add_systems(
                FixedUpdate,
                (
                    request_open_gates_for_agents_within_authored_trigger_distance
                        .run_if(openzt2_world_state::simulation_time::simulation_control_queries::simulation_is_running),
                    tick_armed_gate_automatic_close_timers
                        .after(request_open_gates_for_agents_within_authored_trigger_distance)
                        .run_if(openzt2_world_state::simulation_time::simulation_control_queries::simulation_is_running),
                    apply_requested_gate_state.after(tick_armed_gate_automatic_close_timers),
                )
                    .in_set(GateOperationSet)
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                (
                    project_gate_state_to_ready_animation_presentations,
                    invalidate_path_supports_after_topology_or_terrain_changes,
                    hydrate_authored_path_supports_at_sampled_terrain_height
                        .after(invalidate_path_supports_after_topology_or_terrain_changes),
                )
                    .in_set(FixedGameSet::Cleanup),
            )
            .add_systems(OnExit(GamePhase::InGame), reset_topology_grid);
    }
}

fn detach_removed_fence_endpoints(
    removed: On<Remove<openzt2_zoo_state::topology::topology_graph_types::FenceEdge>>,
    mut commands: Commands,
) {
    use openzt2_zoo_state::topology::topology_graph_types::{FenceEdge, FenceEnd, FenceStart};
    let entity = removed.entity;
    commands.queue(move |world: &mut World| {
        if world.get::<FenceEdge>(entity).is_none() {
            if let Ok(mut entity) = world.get_entity_mut(entity) {
                entity.remove::<(FenceStart, FenceEnd)>();
            }
        }
    });
}

fn despawn_fence_with_removed_endpoint(
    removed: On<
        Remove<(
            openzt2_zoo_state::topology::topology_graph_types::FenceStart,
            openzt2_zoo_state::topology::topology_graph_types::FenceEnd,
        )>,
    >,
    mut commands: Commands,
) {
    use openzt2_zoo_state::topology::topology_graph_types::{FenceEdge, FenceEnd, FenceStart};
    let entity = removed.entity;
    commands.queue(move |world: &mut World| {
        if world.get::<FenceEdge>(entity).is_some()
            && (world.get::<FenceStart>(entity).is_none()
                || world.get::<FenceEnd>(entity).is_none())
        {
            let _ = world.despawn(entity);
        }
    });
}

fn reset_topology_grid(mut grid: ResMut<TopologyGrid>) {
    *grid = default();
}

#[cfg(test)]
mod path_support_hydration_tests;
#[cfg(test)]
mod topology_deletion_transaction_preparation_tests;
#[cfg(test)]
mod topology_edit_application_tests;

#[cfg(test)]
mod topology_fence_definition_test_fixture;

#[cfg(test)]
mod topology_grid_geometry_tests;
#[cfg(test)]
mod topology_index_hydration_tests;
