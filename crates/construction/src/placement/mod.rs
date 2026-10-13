//! Authored object placement over loaded definitions and precise ECS indexes.

mod automatic_object_placement_footprint_hydration;
mod behavior_target_placement_application;
mod biome_automatic_object_placement_edit_preparation;
mod ground_path_object_placement_collision_queries;
mod object_placement_edit_application;
mod object_placement_edit_direction;
mod object_placement_or_relocation_edit_preparation;
mod object_placement_preview_lifecycle;
mod object_placement_preview_permission_projection;
mod object_placement_preview_presentation;
mod object_placement_preview_rotation;
mod object_placement_preview_validation;
mod object_placement_request_routing;
mod placed_object_creation_edit_application;
mod placed_object_footprint_occupancy_index_synchronization;
mod placed_object_relocation_edit_application;
mod placed_object_removal_edit_application;
mod placed_object_removal_edit_preparation;
mod placed_object_terrain_support_reconciliation;

use crate::placement::automatic_object_placement_footprint_hydration::hydrate_automatic_object_placement_footprints_from_loaded_scene_prefabs;
use crate::placement::biome_automatic_object_placement_edit_preparation::hydrate_biome_automatic_placement_state_onto_world_and_terrain_brush_previews;
use crate::placement::biome_automatic_object_placement_edit_preparation::prepare_biome_automatic_object_placement_edits;
use crate::placement::object_placement_edit_application::apply_prepared_object_placement_edits;
use crate::placement::object_placement_or_relocation_edit_preparation::prepare_object_placement_or_relocation_edit;
use crate::placement::object_placement_preview_lifecycle::animate_object_placement_preview_presentation_toward_authoritative_transform;
use crate::placement::object_placement_preview_lifecycle::apply_authored_initial_animal_placement_preview_animation_repetition_policy;
use crate::placement::object_placement_preview_lifecycle::create_active_object_placement_preview;
use crate::placement::object_placement_preview_lifecycle::hydrate_object_placement_preview_prefab_render_tree;
use crate::placement::object_placement_preview_lifecycle::synchronize_object_placement_preview_with_construction_cursor;
use crate::placement::object_placement_preview_permission_projection::project_authoritative_facts_onto_object_placement_previews;
use crate::placement::object_placement_preview_presentation::hydrate_object_placement_preview_materials_from_ui_document;
use crate::placement::object_placement_preview_presentation::project_object_placement_validity_onto_preview_model_tint;
use crate::placement::object_placement_preview_presentation::rebuild_object_placement_preview_footprint_and_grid_decals;
use crate::placement::object_placement_preview_rotation::rotate_active_object_placement_preview_by_authored_increment;
use crate::placement::object_placement_preview_validation::evaluate_requested_object_placement_previews;
use crate::placement::object_placement_preview_validation::request_changed_object_placement_preview_evaluations;
use crate::placement::object_placement_request_routing::cancel_all_active_object_relocations;
use crate::placement::object_placement_request_routing::route_placed_object_removal_requests_to_construction;
use crate::placement::object_placement_request_routing::route_valid_object_placement_requests_to_construction;
use crate::placement::placed_object_footprint_occupancy_index_synchronization::hydrate_placed_object_placement_components;
use crate::placement::placed_object_footprint_occupancy_index_synchronization::rebuild_placed_object_footprint_occupancy_index;
use crate::placement::placed_object_footprint_occupancy_index_synchronization::synchronize_physics_moved_object_footprints_with_occupancy_index;
use crate::placement::placed_object_removal_edit_preparation::prepare_placed_object_removal_edit;
use crate::placement::placed_object_terrain_support_reconciliation::reconcile_static_placed_object_support_after_terrain_changes;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementCellCollectionScratch;
use openzt2_zoo_state::placement::placement_transaction_types::ApplyPreparedObjectPlacementEditRequest;
use openzt2_zoo_state::placement::placement_transaction_types::CommitObjectPlacementPreviewRequest;
use openzt2_zoo_state::placement::placement_transaction_types::EvaluateObjectPlacementPreviewRequest;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementCommitted;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditApplicationAcknowledged;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditPreparationRejected;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditPrepared;
use openzt2_zoo_state::placement::placement_transaction_types::PlacedObjectTerrainSupportEvaluated;
use openzt2_zoo_state::placement::placement_transaction_types::RemovePlacedObjectRequest;

use avian3d::prelude::PhysicsSystems;
use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::construction::FixedConstructionPreparedDomainEditMaterializationSet;
use openzt2_zoo_state::construction::FixedConstructionSet;

/// Registers object-placement data and its explicit schedule order.
pub struct ObjectPlacementPlugin;

use openzt2_zoo_state::placement::ObjectPlacementPreviewUpdateSet;

impl Plugin for ObjectPlacementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            FixedUpdate,
            crate::placement::behavior_target_placement_application::apply_behavior_target_placements
                .in_set(FixedGameSet::Act)
                .after(rebuild_placed_object_footprint_occupancy_index)
                .before(FixedConstructionSet::Apply)
                .run_if(in_state(GamePhase::InGame)),
        );
        app.add_systems(
            FixedUpdate,
            crate::placement::behavior_target_placement_application::settle_behavior_target_placement_payments
                .in_set(FixedGameSet::Cleanup)
                .run_if(in_state(GamePhase::InGame)),
        );
        app.init_resource::<PlacedObjectFootprintOccupancyIndex>()
            .init_resource::<ObjectPlacementCellCollectionScratch>()
            .add_message::<EvaluateObjectPlacementPreviewRequest>()
            .add_message::<CommitObjectPlacementPreviewRequest>()
            .add_message::<RemovePlacedObjectRequest>()
            .add_message::<ObjectPlacementCommitted>()
            .add_message::<ObjectPlacementEditPrepared>()
            .add_message::<ObjectPlacementEditPreparationRejected>()
            .add_message::<ApplyPreparedObjectPlacementEditRequest>()
            .add_message::<ObjectPlacementEditApplicationAcknowledged>()
            .add_message::<PlacedObjectTerrainSupportEvaluated>()
            .add_systems(
                Update,
                (
                    route_valid_object_placement_requests_to_construction,
                    route_placed_object_removal_requests_to_construction,
                    cancel_all_active_object_relocations,
                    create_active_object_placement_preview,
                    hydrate_biome_automatic_placement_state_onto_world_and_terrain_brush_previews,
                    hydrate_automatic_object_placement_footprints_from_loaded_scene_prefabs,
                    hydrate_object_placement_preview_prefab_render_tree,
                    apply_authored_initial_animal_placement_preview_animation_repetition_policy,
                    hydrate_object_placement_preview_materials_from_ui_document,
                    rotate_active_object_placement_preview_by_authored_increment,
                    synchronize_object_placement_preview_with_construction_cursor,
                    project_authoritative_facts_onto_object_placement_previews,
                    request_changed_object_placement_preview_evaluations,
                    evaluate_requested_object_placement_previews,
                    rebuild_object_placement_preview_footprint_and_grid_decals,
                    project_object_placement_validity_onto_preview_model_tint,
                    animate_object_placement_preview_presentation_toward_authoritative_transform,
                )
                    .chain()
                    .in_set(ObjectPlacementPreviewUpdateSet)
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    hydrate_placed_object_placement_components,
                    rebuild_placed_object_footprint_occupancy_index,
                )
                    .chain()
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    prepare_object_placement_or_relocation_edit,
                    prepare_placed_object_removal_edit,
                    prepare_biome_automatic_object_placement_edits,
                )
                    .in_set(FixedConstructionSet::Prepare)
                    .after(
                        openzt2_zoo_state::construction::FixedConstructionTransactionMaterializationSet,
                    )
                    .before(FixedConstructionPreparedDomainEditMaterializationSet)
                    .before(openzt2_zoo_state::construction::ConstructionDomainPreparationCollectionSet)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                apply_prepared_object_placement_edits
                    .in_set(FixedConstructionSet::Apply)
                    .after(openzt2_zoo_state::construction::ConstructionDomainApplicationDispatchSet)
                    .before(openzt2_zoo_state::construction::ConstructionDomainAcknowledgementCollectionSet)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedPostUpdate,
                (
                    reconcile_static_placed_object_support_after_terrain_changes,
                    synchronize_physics_moved_object_footprints_with_occupancy_index,
                )
                    .chain()
                    .after(PhysicsSystems::Writeback)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                OnExit(GamePhase::InGame),
                reset_object_placement_resources,
            );
    }
}

fn reset_object_placement_resources(
    mut index: ResMut<PlacedObjectFootprintOccupancyIndex>,
    mut scratch: ResMut<ObjectPlacementCellCollectionScratch>,
) {
    *index = default();
    *scratch = default();
}

#[cfg(test)]
mod tests;
