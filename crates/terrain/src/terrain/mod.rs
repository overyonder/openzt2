//! Editable terrain projected from live source assets and Bevy images and meshes.

mod defined_terrain_stroke_resolution;
mod ground_path_surface_mask_selection;
mod terrain_biome_detail_projection;
mod terrain_biome_detail_types;
mod terrain_brush_cursor_projection;
mod terrain_brush_interaction;
mod terrain_brush_sample_mutation;
mod terrain_change_tracking_types;
mod terrain_chunk_index_cleanup;
mod terrain_chunk_initial_projection;
mod terrain_chunk_mesh_construction;
mod terrain_chunk_render_refresh;
mod terrain_collision_rebuilding;
mod terrain_edit_application;
mod terrain_edit_preparation;
mod terrain_ground_path_surface_projection;
mod terrain_live_brush_application;
mod terrain_object_placement_flattening_preparation;
mod terrain_presentation_restoration;
mod terrain_sample_change_calculations;
mod terrain_surface_image_composition;
mod terrain_water_caustics;
mod terrain_water_disturbance_presentation;
mod terrain_water_geometric_wave_shader_state;
mod terrain_water_material_binding;
mod terrain_water_mesh_construction;
mod terrain_water_rays;
mod terrain_water_renderer_inputs;
mod terrain_water_renderer_targets;
mod terrain_water_shoreline_geometry;
mod terrain_water_shoreline_presentation;
mod terrain_water_surface_projection;
mod terrain_water_underwater_particles;
mod terrain_water_view_selection;
mod terrain_water_view_transitions;
mod terrain_waterfall_projection;
mod water_regions;

use crate::terrain::defined_terrain_stroke_resolution::resolve_authored_terrain_brush_definitions_into_concrete_strokes;
use crate::terrain::terrain_biome_detail_projection::materialize_terrain_biome_detail_render_trees_within_overhead_camera_range;
use crate::terrain::terrain_biome_detail_projection::project_terrain_biome_details_after_source_or_setting_changes;
use crate::terrain::terrain_brush_cursor_projection::project_terrain_brush_cursor_onto_terrain_surfaces;
use crate::terrain::terrain_brush_interaction::despawn_terrain_brush_previews_after_transaction_preparation;
use crate::terrain::terrain_brush_interaction::update_terrain_brush_preview_and_commit_completed_pointer_strokes;
use crate::terrain::terrain_chunk_initial_projection::apply_changed_effect_quality_to_projected_terrain_chunks;
use crate::terrain::terrain_chunk_initial_projection::validate_index_and_project_loaded_terrain_chunks;
use crate::terrain::terrain_chunk_render_refresh::refresh_changed_terrain_chunk_meshes_and_surface_images;
use crate::terrain::terrain_collision_rebuilding::rebuild_pending_terrain_chunk_collisions_within_tick_budget;
use crate::terrain::terrain_edit_application::apply_committed_terrain_sample_deltas_and_publish_changed_regions;
use crate::terrain::terrain_edit_preparation::prepare_terrain_sample_deltas_for_construction_transactions;
use crate::terrain::terrain_ground_path_surface_projection::mark_terrain_chunks_requiring_ground_path_surface_recomposition;
use crate::terrain::terrain_ground_path_surface_projection::mark_terrain_chunks_under_changed_painted_textures;
use crate::terrain::terrain_ground_path_surface_projection::prepare_terrain_chunk_surface_images;
use crate::terrain::terrain_ground_path_surface_projection::record_discarded_terrain_paint_area;
use crate::terrain::terrain_ground_path_surface_projection::DiscardedTerrainPaintArea;
use crate::terrain::terrain_live_brush_application::apply_held_terrain_brush_dabs_and_accumulate_one_draft_edit;
use crate::terrain::terrain_live_brush_application::roll_back_cancelled_live_terrain_brush_edit;
use crate::terrain::terrain_object_placement_flattening_preparation::prepare_authored_object_placement_terrain_flattening;
use crate::terrain::terrain_presentation_restoration::mark_restored_terrain_chunks_for_presentation_rebuilding;
use crate::terrain::terrain_water_geometric_wave_shader_state::apply_authored_water_impact_events_to_geometric_wave_channels;
use crate::terrain::terrain_water_renderer_inputs::update_authored_terrain_water_renderer_inputs;
use crate::terrain::terrain_water_renderer_targets::initialize_authored_terrain_water_renderer_targets;
use crate::terrain::terrain_water_surface_projection::publish_terrain_water_surfaces;
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
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainPresentationRestored;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainWaterChanged;
use openzt2_zoo_state::terrain::terrain_navigation_change_types::TerrainNavigationChanged;
use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::ActivateTerrainWaterImpactWave;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_world_state::application_schedule::WaterPresentationSet;
use openzt2_world_state::model_render::ModelRenderPresentationSet;
use openzt2_zoo_state::construction::FixedConstructionPreparedDomainEditMaterializationSet;
use openzt2_zoo_state::construction::FixedConstructionSet;
use openzt2_zoo_state::construction::FixedConstructionTransactionMaterializationSet;

/// Registers terrain-owned state and messages. Root integration owns placement in
/// the shared schedules and render sub-app.
pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainIndex>()
            .init_resource::<terrain_chunk_index_cleanup::TerrainChunkIdLookup>()
            .add_observer(terrain_chunk_index_cleanup::index_validated_terrain_chunk)
            .add_observer(terrain_chunk_index_cleanup::remove_discarded_terrain_chunk_from_indexes)
            .add_plugins(terrain_water_rays::view_occlusion::WaterRayOcclusionPlugin)
            .init_resource::<openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterViewState>()
            .init_resource::<TerrainCollisionBudget>()
            .add_message::<DiscardedTerrainPaintArea>()
            .add_message::<terrain_ground_path_surface_projection::DiscardedGroundPathCell>()
            .add_message::<terrain_ground_path_surface_projection::TerrainOverlayInvalidated>()
            .add_observer(terrain_ground_path_surface_projection::record_discarded_ground_path_cell)
            .add_observer(record_discarded_terrain_paint_area)
            .add_observer(water_regions::edit_retention::retain_water_owners_for_terrain_edit)
            .add_observer(water_regions::edit_retention::release_water_owners_from_discarded_terrain_edit)
            .add_message::<TerrainStroke>()
            .add_message::<DefinedTerrainStroke>()
            .add_message::<TerrainChanged>()
            .add_message::<terrain_sample_change_calculations::TerrainRenderInvalidated>()
            .add_observer(apply_held_terrain_brush_dabs_and_accumulate_one_draft_edit)
            .add_observer(apply_committed_terrain_sample_deltas_and_publish_changed_regions)
            .add_message::<TerrainPresentationRestored>()
            .add_message::<TerrainNavigationChanged>()
            .add_message::<TerrainWaterChanged>()
            .add_message::<water_regions::authored_membership::TerrainWaterMembershipChanged>()
            .add_message::<water_regions::authored_membership::WaterRegionFactsDiscarded>()
            .add_observer(water_regions::authored_membership::invalidate_discarded_water_region_facts)
            .add_observer(terrain_water_caustics::scene_projection::invalidate_bounds_after_dependency_discard)
            .add_message::<ActivateTerrainWaterImpactWave>()
            .add_message::<openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::WaterSurfaceSpraySample>()
            .add_message::<openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::WaterSurfaceWakeSample>()
            .add_message::<TerrainEditPrepared>()
            .add_message::<TerrainEditPreparationRejected>()
            .add_message::<CommitTerrainEdit>()
            .add_message::<TerrainEditAcknowledged>()
            .configure_sets(
                Update,
                (WaterPresentationSet::View, WaterPresentationSet::Environment, WaterPresentationSet::Rendering)
                    .chain()
                    .in_set(GameSet::Presentation),
            )
            .add_systems(
                Update,
                (
                    (
                        mark_restored_terrain_chunks_for_presentation_rebuilding,
                        terrain_sample_change_calculations::merge_terrain_render_invalidations
                            .after(mark_restored_terrain_chunks_for_presentation_rebuilding),
                        validate_index_and_project_loaded_terrain_chunks,
                        apply_changed_effect_quality_to_projected_terrain_chunks
                            .after(validate_index_and_project_loaded_terrain_chunks),
                        project_natural_water_regions
                            .after(validate_index_and_project_loaded_terrain_chunks),
                        water_regions::mark_edited_water_targets_pending,
                        rebuild_edited_water_regions
                            .after(project_natural_water_regions)
                            .after(water_regions::mark_edited_water_targets_pending)
                            .run_if(water_regions::water_target_refresh_is_ready),

                        water_regions::edit_retention::reclaim_unretained_retired_water_owners,
                        water_regions::authored_membership::project_terrain_water_membership
                            .after(project_natural_water_regions)
                            .after(water_regions::edit_retention::reclaim_unretained_retired_water_owners),
                        initialize_authored_terrain_water_renderer_targets,
                    ),
                    (
                        terrain_water_renderer_inputs::initialize_authored_terrain_water_region_material_state
                            .after(project_natural_water_regions),
                        terrain_water_surface_projection::prepare_changed_terrain_water_surfaces
                            .after(water_regions::authored_membership::project_terrain_water_membership),
                        publish_terrain_water_surfaces
                            .after(initialize_authored_terrain_water_renderer_targets)
                            .after(terrain_water_surface_projection::prepare_changed_terrain_water_surfaces),
                        terrain_waterfall_projection::prepare_changed_terrain_waterfall_geometry
                            .after(water_regions::authored_membership::project_terrain_water_membership),
                        terrain_waterfall_projection::publish_terrain_waterfall_draws
                            .after(initialize_authored_terrain_water_renderer_targets)
                            .after(terrain_waterfall_projection::prepare_changed_terrain_waterfall_geometry),
                        terrain_water_renderer_inputs::advance_authored_terrain_water_surface_effect_state
                            .after(publish_terrain_water_surfaces)
                            .after(terrain_waterfall_projection::publish_terrain_waterfall_draws)
                            .after(terrain_water_renderer_inputs::initialize_authored_terrain_water_region_material_state),
                        apply_authored_water_impact_events_to_geometric_wave_channels
                            .after(terrain_water_renderer_inputs::advance_authored_terrain_water_surface_effect_state),
                        terrain_water_view_selection::sample_terrain_water_camera_pose
                            .after(openzt2_player_controls::camera::ZooCameraPoseUpdateSystems),
                        terrain_water_renderer_inputs::project_authored_terrain_water_surface_coverage
                            .after(terrain_water_view_selection::sample_terrain_water_camera_pose)
                            .after(apply_authored_water_impact_events_to_geometric_wave_channels),
                        terrain_water_view_selection::update_camera_terrain_water_membership
                            .after(terrain_water_view_selection::sample_terrain_water_camera_pose)
                            .after(water_regions::authored_membership::project_terrain_water_membership),
                        terrain_water_view_selection::update_terrain_water_view_selection
                            .after(terrain_water_renderer_inputs::project_authored_terrain_water_surface_coverage)
                            .after(terrain_water_view_selection::update_camera_terrain_water_membership),
                        terrain_water_view_transitions::advance_terrain_water_region_fades
                            .after(terrain_water_view_selection::update_terrain_water_view_selection),
                    ),
                )
                    .in_set(WaterPresentationSet::View),
            )
            .add_systems(
                Update,
                terrain_water_renderer_targets::reconcile_water_material_render_views
                    .after(publish_terrain_water_surfaces)
                    .in_set(WaterPresentationSet::View),
            )
            .add_systems(
                Update,
                (
                    update_authored_terrain_water_renderer_inputs,
                    terrain_water_disturbance_presentation::present_authored_water_surface_spray_samples,
                    (
                        terrain_water_caustics::systems::advance_terrain_caustic_animation
                            .after(terrain_water_disturbance_presentation::present_authored_water_surface_spray_samples),
                        terrain_water_caustics::systems::prepare_changed_terrain_caustic_ground_meshes,
                        terrain_water_caustics::systems::project_changed_terrain_caustic_ground_passes
                            .after(terrain_water_caustics::systems::prepare_changed_terrain_caustic_ground_meshes),
                        terrain_water_caustics::systems::update_terrain_caustic_view_bindings
                            .after(update_authored_terrain_water_renderer_inputs)
                            .after(terrain_water_caustics::systems::advance_terrain_caustic_animation)
                            .after(terrain_water_caustics::systems::project_changed_terrain_caustic_ground_passes),
                    ),
                    (
                        terrain_water_shoreline_geometry::projection::project_changed_terrain_shoreline_geometry,
                        terrain_water_shoreline_presentation::initialize_terrain_shoreline_material_animation,
                        terrain_water_shoreline_presentation::advance_terrain_shoreline_material_animation
                            .after(terrain_water_shoreline_presentation::initialize_terrain_shoreline_material_animation),
                        terrain_water_shoreline_presentation::submit_prepared_terrain_shoreline_mesh_groups
                            .after(terrain_water_shoreline_geometry::projection::project_changed_terrain_shoreline_geometry)
                            .after(terrain_water_shoreline_presentation::advance_terrain_shoreline_material_animation),
                        terrain_water_shoreline_presentation::update_terrain_shoreline_material_animation_bindings
                            .after(terrain_water_shoreline_presentation::submit_prepared_terrain_shoreline_mesh_groups),
                    ),
                    (
                        terrain_water_rays::systems::maintain_terrain_water_ray_effects,
                        terrain_water_rays::systems::advance_terrain_water_ray_materials,
                        terrain_water_rays::presentation::update_terrain_water_ray_presentation,
                    ).chain(),
                    (
                        terrain_water_disturbance_presentation::wake_mesh_emission::advance_water_wake_mesh_lifetimes,
                        terrain_water_disturbance_presentation::wake_mesh_emission::present_water_wake_samples
                            .after(terrain_water_caustics::systems::advance_terrain_caustic_animation),
                        terrain_water_disturbance_presentation::wake_mesh_emission::update_water_wake_wave_uniforms,
                    ).chain(),
                    (
                        terrain_water_underwater_particles::runtime::synchronize_region_underwater_particle_effects
                            .after(terrain_water_disturbance_presentation::wake_mesh_emission::present_water_wake_samples),
                        terrain_water_underwater_particles::runtime::emit_region_underwater_particles
                            .after(terrain_water_underwater_particles::runtime::project_region_suspension_volumes),
                        terrain_water_underwater_particles::runtime::update_underwater_particle_visibility,
                    ).chain(),
                    terrain_water_underwater_particles::runtime::project_region_suspension_volumes,
                    advance_authored_terrain_waterfall_decal_detail_texture_coordinates,
                    (
                        terrain_brush_cursor_projection::resolve_terrain_brush_cursor_shape,
                        project_terrain_brush_cursor_onto_terrain_surfaces
                            .before(refresh_changed_terrain_chunk_meshes_and_surface_images)
                            .after(terrain_brush_cursor_projection::resolve_terrain_brush_cursor_shape),
                        refresh_changed_terrain_chunk_meshes_and_surface_images,
                        project_terrain_biome_details_after_source_or_setting_changes
                            .after(refresh_changed_terrain_chunk_meshes_and_surface_images),
                        materialize_terrain_biome_detail_render_trees_within_overhead_camera_range
                            .after(project_terrain_biome_details_after_source_or_setting_changes),
                    ),
                    mark_terrain_chunks_requiring_ground_path_surface_recomposition,
                    mark_terrain_chunks_under_changed_painted_textures,
                    terrain_ground_path_surface_projection::merge_terrain_overlay_invalidations
                        .after(mark_terrain_chunks_requiring_ground_path_surface_recomposition)
                        .after(mark_terrain_chunks_under_changed_painted_textures),
                    prepare_terrain_chunk_surface_images
                        .after(refresh_changed_terrain_chunk_meshes_and_surface_images)
                        .after(terrain_ground_path_surface_projection::merge_terrain_overlay_invalidations),
                    terrain_ground_path_surface_projection::publish_terrain_chunk_surface_images
                        .after(prepare_terrain_chunk_surface_images),
                )
                    .in_set(WaterPresentationSet::Rendering),
            )
            .add_systems(
                PostUpdate,
                (
                    terrain_water_caustics::scene_projection::project_scene_caustic_targets
                        .after(openzt2_world_state::model_render::ModelRenderPresentationSet::LevelOfDetail),
                    terrain_water_caustics::scene_projection::project_scene_caustic_passes
                        .after(terrain_water_caustics::scene_projection::project_scene_caustic_targets)
                        .after(openzt2_game_assets::material::runtime::texture_coordinate_animation::TextureCoordinateAnimationSet)
                        .after(ModelRenderPresentationSet::SourceMaterials)
                        .in_set(ModelRenderPresentationSet::VisibilityInputs),
                    terrain_water_caustics::scene_projection::update_scene_caustic_bindings
                        .after(terrain_water_caustics::scene_projection::project_scene_caustic_passes)
                        .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
                    bevy::transform::systems::propagate_transforms_for::<With<
                        openzt2_zoo_state::terrain::terrain_water_caustic_types::TerrainWaterCausticScenePass,
                    >>
                        .after(terrain_water_caustics::scene_projection::project_scene_caustic_passes)
                        .before(bevy::camera::visibility::VisibilitySystems::CheckVisibility),
                ),
            )
            .add_systems(
                Update,
                (
                    update_terrain_brush_preview_and_commit_completed_pointer_strokes.after(openzt2_zoo_state::construction::ConstructionCursorUpdateSet),
                    resolve_authored_terrain_brush_definitions_into_concrete_strokes,
                    terrain_live_brush_application::dispatch_held_terrain_strokes,
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
                terrain_edit_application::dispatch_committed_terrain_edits
                    .after(openzt2_zoo_state::construction::ConstructionDomainApplicationDispatchSet)
                    .after(openzt2_zoo_state::construction::ConstructionFailureRollbackDispatchSet)
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

fn reset_terrain_index(
    mut index: ResMut<TerrainIndex>,
    mut ids: ResMut<terrain_chunk_index_cleanup::TerrainChunkIdLookup>,
) {
    *index = default();
    ids.0.clear();
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
