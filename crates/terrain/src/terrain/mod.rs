//! Editable terrain projected from live source assets and Bevy images and meshes.

mod defined_terrain_stroke_resolution;
mod ground_path_surface_mask_selection;
mod terrain_biome_detail_projection;
mod terrain_biome_detail_types;
mod terrain_brush_cursor_projection;
mod terrain_brush_interaction;
mod terrain_brush_preview_accumulation;
mod terrain_brush_sample_mutation;
mod terrain_change_tracking_types;
mod terrain_chunk_index_cleanup;
mod terrain_chunk_initial_projection;
mod terrain_chunk_mesh_construction;
mod terrain_chunk_render_refresh;
mod terrain_collision_rebuilding;
mod terrain_edit_application;
mod terrain_edit_preparation;
pub mod terrain_ground_path_surface_projection;
mod terrain_live_brush_application;
mod terrain_object_placement_flattening_preparation;
mod terrain_sample_change_calculations;
mod terrain_surface_image_composition;
mod terrain_water_geometric_wave_shader_state;
mod terrain_water_material_binding;
mod terrain_water_mesh_construction;
mod terrain_water_renderer_inputs;
mod terrain_water_renderer_targets;
mod terrain_water_surface_projection;
mod terrain_waterfall_projection;
mod water_regions;

use crate::terrain::defined_terrain_stroke_resolution::resolve_authored_terrain_brush_definitions_into_concrete_strokes;
use crate::terrain::terrain_biome_detail_projection::materialize_terrain_biome_detail_render_trees_within_overhead_camera_range;
use crate::terrain::terrain_biome_detail_projection::project_terrain_biome_details_after_source_or_setting_changes;
use crate::terrain::terrain_brush_cursor_projection::project_terrain_brush_cursor_onto_terrain_surfaces;
use crate::terrain::terrain_brush_interaction::despawn_terrain_brush_previews_after_transaction_preparation;
use crate::terrain::terrain_brush_interaction::update_terrain_brush_preview_and_commit_completed_pointer_strokes;
use crate::terrain::terrain_brush_preview_accumulation::accumulate_terrain_stroke_dabs_into_brush_preview;
use crate::terrain::terrain_chunk_index_cleanup::remove_despawned_terrain_chunks_from_spatial_index;
use crate::terrain::terrain_chunk_initial_projection::apply_changed_effect_quality_to_projected_terrain_chunks;
use crate::terrain::terrain_chunk_initial_projection::validate_index_and_project_loaded_terrain_chunks;
use crate::terrain::terrain_chunk_render_refresh::refresh_changed_terrain_chunk_meshes_and_surface_images;
use crate::terrain::terrain_collision_rebuilding::rebuild_pending_terrain_chunk_collisions_within_tick_budget;
use crate::terrain::terrain_edit_application::apply_committed_terrain_sample_deltas_and_publish_changed_regions;
use crate::terrain::terrain_edit_preparation::prepare_terrain_sample_deltas_for_construction_transactions;
use crate::terrain::terrain_ground_path_surface_projection::mark_terrain_chunks_requiring_ground_path_surface_recomposition;
use crate::terrain::terrain_ground_path_surface_projection::recompose_ground_path_surfaces_into_terrain_chunk_images;
use crate::terrain::terrain_ground_path_surface_projection::retain_ground_path_surface_image_dependencies;
use crate::terrain::terrain_ground_path_surface_projection::GroundPathSurfaceImageDependencies;
use crate::terrain::terrain_live_brush_application::apply_held_terrain_brush_dabs_and_accumulate_one_draft_edit;
use crate::terrain::terrain_live_brush_application::roll_back_cancelled_live_terrain_brush_edit;
use crate::terrain::terrain_object_placement_flattening_preparation::prepare_authored_object_placement_terrain_flattening;
use crate::terrain::terrain_water_geometric_wave_shader_state::apply_authored_water_impact_events_to_geometric_wave_channels;
use crate::terrain::terrain_water_renderer_inputs::update_authored_terrain_water_renderer_inputs;
use crate::terrain::terrain_water_renderer_targets::initialize_authored_terrain_water_renderer_targets;
use crate::terrain::terrain_water_surface_projection::project_changed_terrain_water_surfaces_and_waterfalls;
use crate::terrain::terrain_waterfall_projection::advance_authored_terrain_waterfall_decal_detail_texture_coordinates;
use crate::terrain::water_regions::project_natural_water_regions;
use crate::terrain::water_regions::rebuild_edited_water_regions;
use bevy::prelude::*;
use openzt2_zoo_state::terrain::terrain_brush_types::DefinedTerrainStroke;
use openzt2_zoo_state::terrain::terrain_brush_types::TerrainStroke;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_collision_rebuilding_types::TerrainCollisionBudget;
use openzt2_zoo_state::terrain::terrain_edit_types::CommitTerrainEdit;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEditAcknowledged;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEditPreparationRejected;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEditPrepared;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainWaterChanged;
use openzt2_zoo_state::terrain::terrain_navigation_change_types::TerrainNavigationChanged;
use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::ActivateTerrainWaterImpactWave;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::construction::FixedConstructionPreparedDomainEditMaterializationSet;
use openzt2_zoo_state::construction::FixedConstructionSet;
use openzt2_zoo_state::construction::FixedConstructionTransactionMaterializationSet;

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
                        .after(openzt2_player_controls::camera::ZooCameraPoseUpdateSystems),
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
                    update_terrain_brush_preview_and_commit_completed_pointer_strokes.after(openzt2_zoo_state::construction::ConstructionCursorUpdateSet),
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
                    .before(openzt2_zoo_state::construction::ConstructionPreviewCancellationSet)
                    .in_set(GameSet::Ui),
            )
            .add_systems(
                FixedUpdate,
                (
                    prepare_terrain_sample_deltas_for_construction_transactions
                        .after(FixedConstructionTransactionMaterializationSet)
                        .before(FixedConstructionPreparedDomainEditMaterializationSet)
                        .before(openzt2_zoo_state::construction::ConstructionDomainPreparationCollectionSet),
                    prepare_authored_object_placement_terrain_flattening
                        .after(FixedConstructionTransactionMaterializationSet)
                        .before(FixedConstructionPreparedDomainEditMaterializationSet)
                        .before(openzt2_zoo_state::construction::ConstructionDomainPreparationCollectionSet),
                    despawn_terrain_brush_previews_after_transaction_preparation
                        .after(prepare_terrain_sample_deltas_for_construction_transactions),
                )
                    .in_set(FixedConstructionSet::Prepare),
            )
            .add_systems(
                FixedUpdate,
                apply_committed_terrain_sample_deltas_and_publish_changed_regions
                    .after(openzt2_zoo_state::construction::ConstructionDomainApplicationDispatchSet)
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
mod edited_water_eligibility_tests;
#[cfg(test)]
mod terrain_brush_stroke_tests;
#[cfg(test)]
mod terrain_brush_weight_tests;
#[cfg(test)]
mod terrain_edit_rectangle_tests;
#[cfg(test)]
mod terrain_sample_change_tests;
#[cfg(test)]
mod terrain_sampling_tests;
