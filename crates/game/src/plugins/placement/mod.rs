//! Authored object placement over loaded definitions and precise ECS indexes.

mod automatic_object_placement_footprint_hydration;
mod biome_automatic_object_placement_edit_preparation;
mod ground_path_object_placement_collision_queries;
pub(crate) mod object_placement_definition_queries;
mod object_placement_edit_application;
mod object_placement_edit_direction;
mod object_placement_or_relocation_edit_preparation;
mod object_placement_preview_lifecycle;
mod object_placement_preview_permission_projection;
mod object_placement_preview_presentation;
mod object_placement_preview_rotation;
mod object_placement_preview_validation;
mod object_placement_request_routing;
pub mod object_placement_validation;
mod placed_object_creation_edit_application;
mod placed_object_footprint_occupancy_index_synchronization;
mod placed_object_relocation_edit_application;
mod placed_object_removal_edit_application;
mod placed_object_removal_edit_preparation;
mod placed_object_terrain_support_reconciliation;
pub mod placed_object_types;
mod placement_occupancy_index;
pub mod placement_preview_types;
pub mod placement_transaction_types;

use automatic_object_placement_footprint_hydration::hydrate_automatic_object_placement_footprints_from_loaded_scene_prefabs;
use biome_automatic_object_placement_edit_preparation::{
    hydrate_biome_automatic_placement_state_onto_world_and_terrain_brush_previews,
    prepare_biome_automatic_object_placement_edits,
};
use object_placement_edit_application::apply_prepared_object_placement_edits;
use object_placement_or_relocation_edit_preparation::prepare_object_placement_or_relocation_edit;
use object_placement_preview_lifecycle::{
    animate_object_placement_preview_presentation_toward_authoritative_transform,
    create_active_object_placement_preview, hydrate_object_placement_preview_prefab_render_tree,
    synchronize_object_placement_preview_with_construction_cursor,
};
use object_placement_preview_permission_projection::project_authoritative_facts_onto_object_placement_previews;
use object_placement_preview_presentation::{
    hydrate_object_placement_preview_materials_from_ui_document,
    project_object_placement_validity_onto_preview_model_tint,
    rebuild_object_placement_preview_footprint_and_grid_decals,
};
use object_placement_preview_rotation::rotate_active_object_placement_preview_by_authored_increment;
use object_placement_preview_validation::{
    evaluate_requested_object_placement_previews,
    request_changed_object_placement_preview_evaluations,
};
use object_placement_request_routing::{
    cancel_all_active_object_relocations, route_placed_object_removal_requests_to_construction,
    route_valid_object_placement_requests_to_construction,
};
use placed_object_footprint_occupancy_index_synchronization::{
    hydrate_placed_object_placement_components, rebuild_placed_object_footprint_occupancy_index,
    synchronize_physics_moved_object_footprints_with_occupancy_index,
};
use placed_object_removal_edit_preparation::prepare_placed_object_removal_edit;
use placed_object_terrain_support_reconciliation::reconcile_static_placed_object_support_after_terrain_changes;
use placed_object_types::{
    PhysicsMovedPlacedObjectFootprint, PlacedObjectAuthoredEntrance,
    PlacedObjectDefinitionReference, PlacedObjectFootprintOccupancy,
};
use placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;
use placement_preview_types::{
    ObjectPlacementCellCollectionScratch, ObjectPlacementPrefabSource, ObjectPlacementPreviewDecal,
    ObjectPlacementPreviewMaterials, ObjectPlacementPreviewModelPresentation,
    ObjectPlacementPreviewMotion, ObjectPlacementPreviewOwner,
    ObjectPlacementPreviewPermissionFacts, ObjectPlacementPreviewPrefabHydrated,
    ObjectPlacementPreviewRenderable, ObjectPlacementPreviewRequest,
    ObjectPlacementPreviewVisualState, PlacementRotationRequest, RelocatingPlacedObject,
};
use placement_transaction_types::{
    ApplyPreparedObjectPlacementEditRequest, CommitObjectPlacementPreviewRequest,
    EvaluateObjectPlacementPreviewRequest, ObjectPlacementCommitted,
    ObjectPlacementEditApplicationAcknowledged, ObjectPlacementEditPreparationRejected,
    ObjectPlacementEditPrepared, PlacedObjectRelocationSource, PlacedObjectTerrainSupportEvaluated,
    PlacedObjectTerrainSupportOutcome, PreparedObjectPlacementEdit,
    PreparedObjectPlacementEditMutationKind, PreparedObjectPlacementMutation,
    RemovePlacedObjectRequest,
};

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::{FixedGameSet, GameSet};
use crate::plugins::construction::{
    FixedConstructionPreparedDomainEditMaterializationSet, FixedConstructionSet,
};
use avian3d::prelude::PhysicsSystems;
use bevy::prelude::*;

/// Registers object-placement data and its explicit schedule order.
pub struct ObjectPlacementPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ObjectPlacementPreviewUpdateSet;

impl Plugin for ObjectPlacementPlugin {
    fn build(&self, app: &mut App) {
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
                        crate::plugins::construction::FixedConstructionTransactionMaterializationSet,
                    )
                    .before(FixedConstructionPreparedDomainEditMaterializationSet)
                    .before(
                        crate::plugins::construction::construction_domain_preparation_collection::collect_construction_domain_preparation_results,
                    )
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                apply_prepared_object_placement_edits
                    .in_set(FixedConstructionSet::Apply)
                    .after(
                        crate::plugins::construction::construction_domain_application_dispatch::dispatch_authorized_construction_application_to_participating_domains,
                    )
                    .before(
                        crate::plugins::construction::construction_domain_acknowledgement_collection::collect_construction_domain_application_acknowledgements,
                    )
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
