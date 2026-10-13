//! Fence, path, gate, and portal facts stored directly in the live ECS.

pub mod fence_anchor_press_diagnostic_types;
mod fence_construction_preview_presentation;
mod fence_segment_prefab_refresh;
mod fence_segment_prefab_selection;
pub mod fence_terrain_endpoint_skew_presentation;
mod gate_automatic_operation;
mod gate_state_application;
pub mod path_curb_presentation;
mod path_support_hydration;
mod path_surface_presentation;
mod tank_boundary_projection;
mod tank_boundary_types;
pub mod topology_construction_interaction;
mod topology_construction_planning;
pub mod topology_construction_preview_types;
mod topology_construction_transaction_preparation;
mod topology_definition_hydration;
mod topology_deletion_transaction_preparation;
mod topology_edit_application;
mod topology_edit_outcome_messages;
mod topology_index_hydration;
mod topology_prefab_presentation;
pub mod topology_presentation_types;

use crate::topology::fence_construction_preview_presentation::project_fence_construction_preview_validity_onto_prefab_model_tints;
use crate::topology::fence_construction_preview_presentation::reconcile_fence_construction_preview_presentation_segments;
use crate::topology::fence_segment_prefab_refresh::refresh_fence_segment_prefabs_after_topology_index_changes;
use crate::topology::fence_terrain_endpoint_skew_presentation::fit_fence_presentations_to_terrain_endpoint_heights;
use crate::topology::gate_automatic_operation::advance_gate_automatic_close_countdowns;
use crate::topology::gate_automatic_operation::request_open_gates_for_agents_within_authored_trigger_distance;
use crate::topology::gate_state_application::apply_requested_gate_state;
use crate::topology::gate_state_application::attach_authored_animation_sets_to_resolved_gate_models;
use crate::topology::gate_state_application::project_gate_state_to_ready_animation_presentations;
use crate::topology::path_curb_presentation::hydrate_authored_path_curb_prefab_presentations;
use crate::topology::path_curb_presentation::invalidate_path_curb_presentations_after_topology_or_terrain_changes;
use crate::topology::path_support_hydration::hydrate_authored_path_supports_at_sampled_terrain_height;
use crate::topology::path_support_hydration::invalidate_path_supports_after_topology_or_terrain_changes;
use crate::topology::path_support_hydration::remove_path_supports_after_their_owning_path_is_removed;
use crate::topology::path_surface_presentation::fit_ground_path_tiles_to_authored_terrain;
use crate::topology::path_surface_presentation::hydrate_elevated_path_surface_mesh_and_material_presentations;
use crate::topology::path_surface_presentation::invalidate_ground_path_terrain_fitting_after_terrain_changes;
use crate::topology::path_surface_presentation::ElevatedPathSurfacePresentationAssets;
use crate::topology::tank_boundary_projection::remove_tank_boundary_relationship_entities_with_missing_endpoints;
use crate::topology::tank_boundary_projection::synchronize_tank_boundary_relationships_after_habitat_region_changes;
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
use crate::topology::topology_index_hydration::index_loaded_topology_and_remove_duplicate_or_invalid_entities;
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

use crate::construction::construction_domain_preparation_collection::collect_construction_domain_preparation_results;
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
        app.init_resource::<TopologyIndex>()
            .init_resource::<TopologyGrid>()
            .init_resource::<ElevatedPathSurfacePresentationAssets>()
            .add_message::<PlaceFence>()
            .add_message::<PlacePath>()
            .add_message::<SetGateState>()
            .add_message::<TopologyChanged>()
            .add_message::<TopologyEditPrepared>()
            .add_message::<TopologyEditPreparationRejected>()
            .add_message::<CommitTopologyEdit>()
            .add_message::<TopologyEditAcknowledged>()
            .add_systems(
                OnEnter(GamePhase::InGame),
                index_loaded_topology_and_remove_duplicate_or_invalid_entities,
            )
            .add_systems(
                Update,
                (
                    restart_fence_preview_from_committed_endpoint,
                    restart_path_preview_from_committed_endpoint,
                    drive_selected_topology_placement_pointer_sequence,
                    apply_fence_placement_requests_to_construction_previews,
                    apply_path_placement_requests_to_construction_previews,
                    evaluate_topology_construction_preview_validity,
                )
                    .chain()
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                (
                    hydrate_save_loaded_topology_definition_facts,
                    reconcile_fence_construction_preview_presentation_segments,
                    invalidate_ground_path_terrain_fitting_after_terrain_changes,
                    fit_ground_path_tiles_to_authored_terrain,
                    hydrate_elevated_path_surface_mesh_and_material_presentations,
                    invalidate_path_curb_presentations_after_topology_or_terrain_changes,
                    hydrate_authored_path_curb_prefab_presentations,
                    refresh_fence_segment_prefabs_after_topology_index_changes,
                    hydrate_topology_prefab_presentations,
                    attach_authored_animation_sets_to_resolved_gate_models,
                    fit_fence_presentations_to_terrain_endpoint_heights,
                    project_fence_construction_preview_validity_onto_prefab_model_tints,
                )
                    .chain()
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                FixedUpdate,
                (
                    prepare_topology_construction_transaction
                        .after(
                            openzt2_zoo_state::construction::FixedConstructionTransactionMaterializationSet,
                        )
                        .before(FixedConstructionPreparedDomainEditMaterializationSet)
                        .before(collect_construction_domain_preparation_results),
                    prepare_topology_deletion_transaction
                        .after(
                            openzt2_zoo_state::construction::FixedConstructionTransactionMaterializationSet,
                        )
                        .before(FixedConstructionPreparedDomainEditMaterializationSet)
                        .before(collect_construction_domain_preparation_results),
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
                    advance_gate_automatic_close_countdowns
                        .run_if(openzt2_world_state::simulation_time::simulation_control_queries::simulation_is_running),
                    apply_requested_gate_state,
                )
                    .chain()
                    .in_set(GateOperationSet)
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                (
                    project_gate_state_to_ready_animation_presentations,
                    synchronize_tank_boundary_relationships_after_habitat_region_changes,
                    remove_tank_boundary_relationship_entities_with_missing_endpoints,
                    invalidate_path_supports_after_topology_or_terrain_changes,
                    hydrate_authored_path_supports_at_sampled_terrain_height,
                    remove_path_supports_after_their_owning_path_is_removed,
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup),
            )
            .add_systems(OnExit(GamePhase::InGame), reset_topology_index);
    }
}

fn reset_topology_index(mut index: ResMut<TopologyIndex>, mut grid: ResMut<TopologyGrid>) {
    *index = default();
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
