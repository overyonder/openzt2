//! Authored object placement over loaded definitions and precise ECS indexes.

use bevy::ecs::schedule::common_conditions::any_with_component;
mod behavior_target_placement_application;
mod biome_automatic_object_placement_edit_preparation;
mod ground_path_placement_stomp;
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
mod placed_object_destruction;
mod placed_object_footprint_occupancy_index_synchronization;
mod placed_object_relocation_edit_application;
mod placed_object_removal_edit_application;
mod placed_object_removal_edit_preparation;
mod placed_object_terrain_support_reconciliation;
mod scene_placement_footprint_indexing;
pub(crate) mod stomped_object_edit_preparation;

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
use crate::placement::object_placement_preview_validation::evaluate_dirty_object_placement_previews;
use crate::placement::object_placement_preview_validation::mark_changed_object_placement_previews_dirty;
use crate::placement::object_placement_request_routing::cancel_all_active_object_relocations;
use crate::placement::object_placement_request_routing::route_placed_object_removal_requests_to_construction;
use crate::placement::object_placement_request_routing::route_valid_object_placement_requests_to_construction;
use crate::placement::placed_object_footprint_occupancy_index_synchronization::hydrate_placed_object_placement_components;
use crate::placement::placed_object_footprint_occupancy_index_synchronization::synchronize_physics_moved_object_footprints_with_occupancy_index;
use crate::placement::placed_object_footprint_occupancy_index_synchronization::synchronize_placed_footprints;
use crate::placement::placed_object_removal_edit_preparation::prepare_placed_object_removal_edit;
use crate::placement::placed_object_terrain_support_reconciliation::reconcile_static_placed_object_support_after_terrain_changes;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementCellCollectionScratch;
use openzt2_zoo_state::placement::placement_transaction_types::ApplyPreparedObjectPlacementEditRequest;
use openzt2_zoo_state::placement::placement_transaction_types::CommitObjectPlacementPreviewRequest;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementCommitted;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditApplicationAcknowledged;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditPreparationRejected;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditPrepared;
use openzt2_zoo_state::placement::placement_transaction_types::PlacedObjectTerrainSupportEvaluated;
use openzt2_zoo_state::placement::placement_transaction_types::RemovePlacedObjectRequest;

use avian3d::prelude::PhysicsSystems;
use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::world_definitions::WorldDefinitionPublicationSet;
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
                .before(FixedConstructionSet::Apply)
                .run_if(in_state(GamePhase::InGame)),
        );
        app.add_message::<openzt2_zoo_state::placement::placement_transaction_types::DestroyPlacedObject>()
            .add_systems(
                FixedUpdate,
                (
                    placed_object_destruction::mark_buildings_requested_for_destruction,
                    placed_object_destruction::resolve_requested_building_destruction_assets
                        .after(placed_object_destruction::mark_buildings_requested_for_destruction),
                    placed_object_destruction::ruin_destroyed_placed_objects
                        .after(placed_object_destruction::resolve_requested_building_destruction_assets),
                )
                    .in_set(FixedGameSet::Act)
                    .before(FixedConstructionSet::Apply)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                PreUpdate,
                placed_object_destruction::mark_waiting_building_destruction_assets_ready
                    .after(WorldDefinitionPublicationSet)
                    .after(scene_placement_footprint_indexing::index_scene_placement_footprints)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                placed_object_destruction::fly_rubble_fragments
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
            .add_message::<openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprintChanged>()
            .add_message::<CommitObjectPlacementPreviewRequest>()
            .add_message::<RemovePlacedObjectRequest>()
            .init_resource::<openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints>()
            .add_systems(PreUpdate, scene_placement_footprint_indexing::index_scene_placement_footprints.after(bevy::asset::AssetTrackingSystems))
            .add_message::<ObjectPlacementCommitted>()
            .add_message::<ObjectPlacementEditPrepared>()
            .add_message::<ObjectPlacementEditPreparationRejected>()
            .add_message::<ApplyPreparedObjectPlacementEditRequest>()
            .add_message::<ObjectPlacementEditApplicationAcknowledged>()
            .add_message::<PlacedObjectTerrainSupportEvaluated>()
            .add_systems(
                Update,
                (
                    route_placed_object_removal_requests_to_construction,
                    cancel_all_active_object_relocations,
                    create_active_object_placement_preview
                        .after(crate::construction::construction_tool_and_action_routing::select_construction_tool),
                    biome_automatic_object_placement_edit_preparation::hydrate_biome_automatic_placement_state_onto_world_and_terrain_brush_previews,
                    hydrate_object_placement_preview_prefab_render_tree.after(create_active_object_placement_preview),
                    apply_authored_initial_animal_placement_preview_animation_repetition_policy
                        .after(hydrate_object_placement_preview_prefab_render_tree),
                    hydrate_object_placement_preview_materials_from_ui_document,
                    rotate_active_object_placement_preview_by_authored_increment.after(create_active_object_placement_preview),
                    synchronize_object_placement_preview_with_construction_cursor
                        .after(rotate_active_object_placement_preview_by_authored_increment)
                        .after(openzt2_zoo_state::construction::ConstructionCursorUpdateSet),
                    project_authoritative_facts_onto_object_placement_previews
                        .after(synchronize_object_placement_preview_with_construction_cursor)
                        .run_if(any_with_component::<openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPrefabSource>),
                    mark_changed_object_placement_previews_dirty.after(project_authoritative_facts_onto_object_placement_previews),
                    evaluate_dirty_object_placement_previews.after(mark_changed_object_placement_previews_dirty),
                    route_valid_object_placement_requests_to_construction.after(evaluate_dirty_object_placement_previews),
                    rebuild_object_placement_preview_footprint_and_grid_decals
                        .after(evaluate_dirty_object_placement_previews)
                        .after(hydrate_object_placement_preview_materials_from_ui_document),
                    (
                        crate::placement::object_placement_preview_presentation::pulse_object_placement_entrance_arrows
                            .after(rebuild_object_placement_preview_footprint_and_grid_decals),
                        project_object_placement_validity_onto_preview_model_tint
                            .after(evaluate_dirty_object_placement_previews)
                            .after(hydrate_object_placement_preview_prefab_render_tree),
                        crate::placement::object_placement_preview_presentation::hide_alpha_blended_meshes_of_object_placement_previews
                            .after(hydrate_object_placement_preview_prefab_render_tree),
                        animate_object_placement_preview_presentation_toward_authoritative_transform
                            .after(synchronize_object_placement_preview_with_construction_cursor),
                    ),
                )
                    .in_set(ObjectPlacementPreviewUpdateSet)
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                PreUpdate,
                (
                    hydrate_placed_object_placement_components,
                    synchronize_placed_footprints,
                )
                    .chain()
                    .after(WorldDefinitionPublicationSet)
                    .after(scene_placement_footprint_indexing::index_scene_placement_footprints),
            )
            .add_systems(
                PostUpdate,
                crate::construction::capture_confirmed_preview::<object_placement_preview_validation::ObjectPlacementFit>,
            )
            .add_systems(
                FixedUpdate,
                crate::construction::clear_confirmed_previews::<object_placement_preview_validation::ObjectPlacementFit>
                    .after(FixedConstructionSet::Prepare)
                    .before(FixedConstructionSet::Apply),
            )
            .add_systems(
                FixedUpdate,
                (
                    prepare_object_placement_or_relocation_edit,
                    prepare_placed_object_removal_edit,
                    biome_automatic_object_placement_edit_preparation::prepare_biome_automatic_object_placement_edits,
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
                    .after(openzt2_zoo_state::construction::ConstructionFailureRollbackDispatchSet)
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

#[cfg(any(test, feature = "testing"))]
pub mod persistence_test_support;
