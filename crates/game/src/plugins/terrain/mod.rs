//! Editable terrain projected from live source assets and Bevy images and meshes.

mod defined_terrain_stroke_resolution;
pub(crate) mod dry_surface_eligibility;
mod ground_path_surface_mask_selection;
mod terrain_biome_detail_projection;
mod terrain_biome_detail_types;
mod terrain_brush_cursor_projection;
mod terrain_brush_interaction;
pub(crate) mod terrain_brush_interaction_state;
mod terrain_brush_preview_accumulation;
mod terrain_brush_sample_mutation;
pub(crate) mod terrain_brush_types;
mod terrain_change_tracking_types;
pub mod terrain_chunk_identity_type;
mod terrain_chunk_index_cleanup;
mod terrain_chunk_initial_projection;
mod terrain_chunk_mesh_construction;
pub(crate) mod terrain_chunk_presentation_types;
mod terrain_chunk_render_refresh;
pub(crate) mod terrain_chunk_types;
mod terrain_collision_rebuilding;
mod terrain_collision_rebuilding_types;
mod terrain_edit_application;
pub mod terrain_edit_data_types;
mod terrain_edit_preparation;
mod terrain_edit_rectangle_operations;
pub(crate) mod terrain_edit_types;
pub(crate) mod terrain_fitted_surface_mesh_construction;
mod terrain_ground_path_surface_projection;
mod terrain_live_brush_application;
pub(crate) mod terrain_navigation_cell_queries;
pub mod terrain_navigation_change_types;
mod terrain_object_placement_flattening_preparation;
mod terrain_sample_change_calculations;
mod terrain_sample_grid_queries;
mod terrain_surface_image_composition;
pub(crate) mod terrain_water_geometric_wave_shader_state;
mod terrain_water_material_binding;
mod terrain_water_mesh_construction;
pub mod terrain_water_presentation_types;
mod terrain_water_renderer_inputs;
mod terrain_water_renderer_targets;
pub(crate) mod terrain_water_renderer_types;
mod terrain_water_surface_projection;
mod terrain_waterfall_projection;
pub(crate) mod terrain_world_sampling;
pub mod terrain_world_sampling_types;
mod water_regions;

use bevy::prelude::*;
use defined_terrain_stroke_resolution::resolve_authored_terrain_brush_definitions_into_concrete_strokes;
use terrain_biome_detail_projection::{
    materialize_terrain_biome_detail_render_trees_within_overhead_camera_range,
    project_terrain_biome_details_after_source_or_setting_changes,
};
use terrain_brush_cursor_projection::project_terrain_brush_cursor_onto_terrain_surfaces;
use terrain_brush_interaction::{
    despawn_terrain_brush_previews_after_transaction_preparation,
    update_terrain_brush_preview_and_commit_completed_pointer_strokes,
};
use terrain_brush_preview_accumulation::accumulate_terrain_stroke_dabs_into_brush_preview;
use terrain_brush_types::{DefinedTerrainStroke, TerrainStroke};
use terrain_chunk_index_cleanup::remove_despawned_terrain_chunks_from_spatial_index;
use terrain_chunk_initial_projection::{
    apply_changed_effect_quality_to_projected_terrain_chunks,
    validate_index_and_project_loaded_terrain_chunks,
};
use terrain_chunk_render_refresh::refresh_changed_terrain_chunk_meshes_and_surface_images;
use terrain_chunk_types::TerrainIndex;
use terrain_collision_rebuilding::rebuild_pending_terrain_chunk_collisions_within_tick_budget;
use terrain_collision_rebuilding_types::TerrainCollisionBudget;
use terrain_edit_application::apply_committed_terrain_sample_deltas_and_publish_changed_regions;
use terrain_edit_preparation::prepare_terrain_sample_deltas_for_construction_transactions;
use terrain_edit_types::{
    CommitTerrainEdit, TerrainChanged, TerrainEditAcknowledged, TerrainEditPreparationRejected,
    TerrainEditPrepared, TerrainWaterChanged,
};
use terrain_ground_path_surface_projection::{
    mark_terrain_chunks_requiring_ground_path_surface_recomposition,
    recompose_ground_path_surfaces_into_terrain_chunk_images,
    retain_ground_path_surface_image_dependencies, GroundPathSurfaceImageDependencies,
};
use terrain_live_brush_application::{
    apply_held_terrain_brush_dabs_and_accumulate_one_draft_edit,
    roll_back_cancelled_live_terrain_brush_edit,
};
use terrain_navigation_change_types::TerrainNavigationChanged;
use terrain_object_placement_flattening_preparation::prepare_authored_object_placement_terrain_flattening;
use terrain_water_geometric_wave_shader_state::apply_authored_water_impact_events_to_geometric_wave_channels;
use terrain_water_geometric_wave_shader_state::ActivateTerrainWaterImpactWave;
use terrain_water_renderer_inputs::update_authored_terrain_water_renderer_inputs;
use terrain_water_renderer_targets::initialize_authored_terrain_water_renderer_targets;
use terrain_water_surface_projection::project_changed_terrain_water_surfaces_and_waterfalls;
use terrain_waterfall_projection::advance_authored_terrain_waterfall_decal_detail_texture_coordinates;
use water_regions::{project_natural_water_regions, rebuild_edited_water_regions};

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::{FixedGameSet, GameSet};
use crate::plugins::construction::{
    FixedConstructionPreparedDomainEditMaterializationSet, FixedConstructionSet,
    FixedConstructionTransactionMaterializationSet,
};

/// Registers terrain-owned state and messages. Root integration owns placement in
/// the shared schedules and render sub-app.
pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainIndex>()
            .init_resource::<TerrainCollisionBudget>()
            .init_resource::<GroundPathSurfaceImageDependencies>()
            .add_message::<TerrainStroke>()
            .add_message::<DefinedTerrainStroke>()
            .add_message::<TerrainChanged>()
            .add_message::<TerrainNavigationChanged>()
            .add_message::<TerrainWaterChanged>()
            .add_message::<ActivateTerrainWaterImpactWave>()
            .add_message::<TerrainEditPrepared>()
            .add_message::<TerrainEditPreparationRejected>()
            .add_message::<CommitTerrainEdit>()
            .add_message::<TerrainEditAcknowledged>()
            .add_systems(
                Update,
                (
                    validate_index_and_project_loaded_terrain_chunks,
                    apply_changed_effect_quality_to_projected_terrain_chunks,
                    project_natural_water_regions,
                    rebuild_edited_water_regions,
                    initialize_authored_terrain_water_renderer_targets,
                    project_changed_terrain_water_surfaces_and_waterfalls,
                    apply_authored_water_impact_events_to_geometric_wave_channels,
                    update_authored_terrain_water_renderer_inputs
                        .after(crate::plugins::camera::ZooCameraPoseUpdateSystems),
                    advance_authored_terrain_waterfall_decal_detail_texture_coordinates,
                    project_terrain_biome_details_after_source_or_setting_changes,
                    materialize_terrain_biome_detail_render_trees_within_overhead_camera_range,
                    refresh_changed_terrain_chunk_meshes_and_surface_images,
                    retain_ground_path_surface_image_dependencies,
                    mark_terrain_chunks_requiring_ground_path_surface_recomposition,
                    recompose_ground_path_surfaces_into_terrain_chunk_images,
                    project_terrain_brush_cursor_onto_terrain_surfaces,
                    remove_despawned_terrain_chunks_from_spatial_index,
                )
                    .chain()
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                (
                    update_terrain_brush_preview_and_commit_completed_pointer_strokes.after(
                        crate::plugins::construction::construction_cursor_terrain_surface_tracking::update_construction_cursor,
                    ),
                    resolve_authored_terrain_brush_definitions_into_concrete_strokes,
                    accumulate_terrain_stroke_dabs_into_brush_preview,
                    apply_held_terrain_brush_dabs_and_accumulate_one_draft_edit,
                )
                    .chain()
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                roll_back_cancelled_live_terrain_brush_edit
                    .before(crate::plugins::construction::construction_tool_and_action_routing::cancel_preview)
                    .in_set(GameSet::Ui),
            )
            .add_systems(
                FixedUpdate,
                (
                    prepare_terrain_sample_deltas_for_construction_transactions
                        .after(FixedConstructionTransactionMaterializationSet)
                        .before(FixedConstructionPreparedDomainEditMaterializationSet)
                        .before(
                            crate::plugins::construction::construction_domain_preparation_collection::collect_construction_domain_preparation_results,
                        ),
                    prepare_authored_object_placement_terrain_flattening
                        .after(FixedConstructionTransactionMaterializationSet)
                        .before(FixedConstructionPreparedDomainEditMaterializationSet)
                        .before(
                            crate::plugins::construction::construction_domain_preparation_collection::collect_construction_domain_preparation_results,
                        ),
                    despawn_terrain_brush_previews_after_transaction_preparation
                        .after(prepare_terrain_sample_deltas_for_construction_transactions),
                )
                    .in_set(FixedConstructionSet::Prepare),
            )
            .add_systems(
                FixedUpdate,
                apply_committed_terrain_sample_deltas_and_publish_changed_regions
                    .after(
                        crate::plugins::construction::construction_domain_application_dispatch::dispatch_authorized_construction_application_to_participating_domains,
                    )
                    .in_set(FixedConstructionSet::Apply),
            )
            .add_systems(
                FixedUpdate,
                rebuild_pending_terrain_chunk_collisions_within_tick_budget
                    .in_set(FixedGameSet::Cleanup),
            )
            .add_systems(OnExit(GamePhase::InGame), reset_terrain_index);
    }
}

fn reset_terrain_index(mut index: ResMut<TerrainIndex>) {
    *index = default();
}

#[cfg(test)]
mod terrain_brush_weight_tests;
#[cfg(test)]
mod terrain_edit_rectangle_tests;
#[cfg(test)]
mod terrain_sample_change_tests;
#[cfg(test)]
mod terrain_sampling_tests;
