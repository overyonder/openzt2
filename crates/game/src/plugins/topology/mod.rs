//! Fence, path, gate, and portal facts stored directly in the live ECS.

mod fence_construction_preview_presentation;
mod fence_segment_prefab_refresh;
mod fence_segment_prefab_selection;
mod fence_terrain_endpoint_skew_presentation;
mod gate_automatic_operation;
pub(crate) mod gate_operation_types;
mod gate_state_application;
pub(crate) mod ground_path_layout_calculation;
mod path_curb_presentation;
mod path_support_hydration;
mod path_surface_presentation;
mod tank_boundary_projection;
mod tank_boundary_types;
mod topology_construction_interaction;
mod topology_construction_planning;
pub(crate) mod topology_construction_preview_types;
mod topology_construction_transaction_preparation;
mod topology_definition_hydration;
mod topology_deletion_transaction_preparation;
mod topology_edit_application;
mod topology_edit_outcome_messages;
pub(crate) mod topology_edit_types;
pub(crate) mod topology_graph_types;
pub(crate) mod topology_grid_geometry;
mod topology_index_hydration;
mod topology_prefab_presentation;
pub(crate) mod topology_presentation_types;

use fence_construction_preview_presentation::{
    project_fence_construction_preview_validity_onto_prefab_model_tints,
    reconcile_fence_construction_preview_presentation_segments,
};
use fence_segment_prefab_refresh::refresh_fence_segment_prefabs_after_topology_index_changes;
use fence_terrain_endpoint_skew_presentation::fit_fence_presentations_to_terrain_endpoint_heights;
use gate_automatic_operation::{
    advance_gate_automatic_close_countdowns,
    request_open_gates_for_agents_within_authored_trigger_distance,
};
use gate_operation_types::SetGateState;
use gate_state_application::apply_requested_gate_state_and_animation;
use path_curb_presentation::{
    hydrate_authored_path_curb_prefab_presentations,
    invalidate_path_curb_presentations_after_topology_or_terrain_changes,
};
use path_support_hydration::{
    hydrate_authored_path_supports_at_sampled_terrain_height,
    invalidate_path_supports_after_topology_or_terrain_changes,
    remove_path_supports_after_their_owning_path_is_removed,
};
use path_surface_presentation::{
    fit_ground_path_tiles_to_authored_terrain,
    hydrate_elevated_path_surface_mesh_and_material_presentations,
    invalidate_ground_path_terrain_fitting_after_terrain_changes,
    ElevatedPathSurfacePresentationAssets,
};
use tank_boundary_projection::{
    remove_tank_boundary_relationship_entities_with_missing_endpoints,
    synchronize_tank_boundary_relationships_after_habitat_region_changes,
};
use topology_construction_interaction::{
    apply_fence_placement_requests_to_construction_previews,
    apply_path_placement_requests_to_construction_previews,
    drive_selected_topology_placement_pointer_sequence,
    evaluate_topology_construction_preview_validity, restart_fence_preview_from_committed_endpoint,
    restart_path_preview_from_committed_endpoint,
};
use topology_construction_preview_types::{PlaceFence, PlacePath};
use topology_construction_transaction_preparation::prepare_topology_construction_transaction;
use topology_definition_hydration::hydrate_save_loaded_topology_definition_facts;
use topology_deletion_transaction_preparation::prepare_topology_deletion_transaction;
use topology_edit_application::apply_committed_topology_edit;
use topology_edit_types::{
    CommitTopologyEdit, TopologyChanged, TopologyEditAcknowledged, TopologyEditPreparationRejected,
    TopologyEditPrepared,
};
use topology_graph_types::{TopologyGrid, TopologyIndex};
use topology_index_hydration::index_loaded_topology_and_remove_duplicate_or_invalid_entities;
use topology_prefab_presentation::hydrate_topology_prefab_presentations;

use bevy::prelude::*;

use crate::{
    application_lifecycle::GamePhase,
    application_schedule::{FixedGameSet, GameSet},
    plugins::construction::{
        construction_domain_preparation_collection::collect_construction_domain_preparation_results,
        FixedConstructionPreparedDomainEditMaterializationSet, FixedConstructionSet,
    },
};

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
                            crate::plugins::construction::FixedConstructionTransactionMaterializationSet,
                        )
                        .before(FixedConstructionPreparedDomainEditMaterializationSet)
                        .before(collect_construction_domain_preparation_results),
                    prepare_topology_deletion_transaction
                        .after(
                            crate::plugins::construction::FixedConstructionTransactionMaterializationSet,
                        )
                        .before(FixedConstructionPreparedDomainEditMaterializationSet)
                        .before(collect_construction_domain_preparation_results),
                )
                    .in_set(FixedConstructionSet::Prepare),
            )
            .add_systems(
                FixedUpdate,
                apply_committed_topology_edit
                    .after(
                        crate::plugins::construction::construction_domain_application_dispatch::dispatch_authorized_construction_application_to_participating_domains,
                    )
                    .after(
                        crate::plugins::construction::construction_failure_rollback_dispatch::dispatch_construction_failure_rollback_to_changed_domains_once,
                    )
                    .before(
                        crate::plugins::construction::construction_domain_acknowledgement_collection::collect_construction_domain_application_acknowledgements,
                    )
                    .in_set(FixedConstructionSet::Apply),
            )
            .add_systems(
                FixedUpdate,
                (
                    request_open_gates_for_agents_within_authored_trigger_distance,
                    advance_gate_automatic_close_countdowns,
                    apply_requested_gate_state_and_animation,
                )
                    .chain()
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                (
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
