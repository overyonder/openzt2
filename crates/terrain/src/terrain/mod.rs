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
use crate::terrain::terrain_object_placement_flattening_preparation::discard_applied_behavior_placement_terrain_flattening;
use crate::terrain::terrain_object_placement_flattening_preparation::flatten_terrain_under_behavior_placed_objects;
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
use openzt2_world_state::application_schedule::WaterPresentationSet;
use openzt2_zoo_state::construction::FixedConstructionPreparedDomainEditMaterializationSet;
use openzt2_zoo_state::construction::FixedConstructionSet;
use openzt2_zoo_state::construction::FixedConstructionTransactionMaterializationSet;

/// Registers terrain-owned state and messages. Root integration owns placement in
/// the shared schedules and render sub-app.
pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainIndex>()
            .add_plugins(terrain_water_rays::view_occlusion::WaterRayOcclusionPlugin)
            .init_resource::<openzt2_zoo_state::terrain::terrain_water_view_state::TerrainWaterViewState>()
            .init_resource::<TerrainCollisionBudget>()
            .init_resource::<GroundPathSurfaceImageDependencies>()
            .add_message::<TerrainStroke>()
            .add_message::<DefinedTerrainStroke>()
            .add_message::<TerrainChanged>()
            .add_message::<TerrainNavigationChanged>()
            .add_message::<TerrainWaterChanged>()
            .add_message::<ActivateTerrainWaterImpactWave>()
            .add_message::<openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::WaterSurfaceSpraySample>()
            .add_message::<openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::WaterSurfaceWakeSample>()
            .add_message::<TerrainEditPrepared>()
            .add_message::<TerrainEditPreparationRejected>()
            .add_message::<CommitTerrainEdit>()
            .add_message::<openzt2_zoo_state::terrain::terrain_edit_types::FlattenTerrainUnderBehaviorPlacement>()
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
                    validate_index_and_project_loaded_terrain_chunks,
                    apply_changed_effect_quality_to_projected_terrain_chunks,
                    project_natural_water_regions,
                    rebuild_edited_water_regions,
                    water_regions::cleanup_unreferenced_retired_water_region_entities,
                    water_regions::authored_membership::project_terrain_water_membership,
                    initialize_authored_terrain_water_renderer_targets,
                    terrain_water_renderer_inputs::initialize_authored_terrain_water_region_material_state,
                    project_changed_terrain_water_surfaces_and_waterfalls,
                    terrain_water_renderer_inputs::advance_authored_terrain_water_surface_effect_state,
                    apply_authored_water_impact_events_to_geometric_wave_channels,
                    terrain_water_renderer_inputs::update_authored_terrain_water_surface_visibility,
                    terrain_water_view_selection::update_camera_terrain_water_membership,
                    terrain_water_view_selection::update_terrain_water_view_selection,
                    terrain_water_view_transitions::advance_terrain_water_region_fades,
                )
                    .chain()
                    .after(openzt2_player_controls::camera::ZooCameraPoseUpdateSystems)
                    .in_set(WaterPresentationSet::View),
            )
            .add_systems(
                Update,
                (
                    update_authored_terrain_water_renderer_inputs,
                    terrain_water_disturbance_presentation::present_authored_water_surface_spray_samples,
                    terrain_water_caustics::systems::advance_terrain_caustic_animation,
                    terrain_water_caustics::systems::project_changed_terrain_caustic_ground_passes,
                    terrain_water_caustics::systems::update_terrain_caustic_view_bindings,
                    terrain_water_caustics::systems::despawn_caustic_passes_without_owner_or_view,
                    (
                        terrain_water_shoreline_geometry::projection::project_changed_terrain_shoreline_geometry,
                        terrain_water_shoreline_presentation::initialize_terrain_shoreline_material_animation,
                        terrain_water_shoreline_presentation::advance_terrain_shoreline_material_animation,
                        terrain_water_shoreline_presentation::submit_prepared_terrain_shoreline_mesh_groups,
                        terrain_water_shoreline_presentation::update_terrain_shoreline_material_animation_bindings,
                        terrain_water_shoreline_presentation::despawn_terrain_shoreline_surfaces_without_material_owners,
                    ).chain(),
                    (
                        terrain_water_rays::systems::maintain_terrain_water_ray_effects,
                        terrain_water_rays::systems::advance_terrain_water_ray_materials,
                        terrain_water_rays::presentation::update_terrain_water_ray_presentation,
                    ).chain(),
                    (
                        terrain_water_disturbance_presentation::wake_mesh_emission::advance_water_wake_mesh_lifetimes,
                        terrain_water_disturbance_presentation::wake_mesh_emission::present_terrain_stitch_water_wake_samples,
                        terrain_water_disturbance_presentation::wake_mesh_emission::present_geometric_water_wake_samples,
                        terrain_water_disturbance_presentation::wake_mesh_emission::update_water_wake_wave_uniforms,
                    ).chain(),
                    (
                        terrain_water_underwater_particles::runtime::synchronize_region_underwater_particle_effects,
                        terrain_water_underwater_particles::runtime::emit_region_underwater_particles,
                        terrain_water_underwater_particles::runtime::update_underwater_particle_visibility,
                    ).chain(),
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
                    .in_set(WaterPresentationSet::Rendering),
            )
            .add_systems(
                PostUpdate,
                (
                    terrain_water_caustics::scene_projection::project_scene_caustic_targets
                        .after(openzt2_world_state::model_render::ModelRenderPresentationSet::LevelOfDetail),
                    terrain_water_caustics::scene_projection::project_scene_caustic_passes,
                    terrain_water_caustics::scene_projection::update_scene_caustic_bindings,
                    bevy::transform::systems::propagate_transforms_for::<With<
                        openzt2_zoo_state::terrain::terrain_water_caustic_types::TerrainWaterCausticScenePass,
                    >>,
                )
                    .chain()
                    .after(openzt2_world_state::model_render::ModelRenderPresentationSet::MaterialProjection)
                    .after(openzt2_game_assets::material::runtime::texture_coordinate_animation::TextureCoordinateAnimationSet)
                    .before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
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
                (
                    flatten_terrain_under_behavior_placed_objects,
                    apply_committed_terrain_sample_deltas_and_publish_changed_regions,
                )
                    .chain()
                    .after(openzt2_zoo_state::construction::ConstructionDomainApplicationDispatchSet)
                    .in_set(FixedConstructionSet::Apply),
            )
            .add_systems(
                FixedUpdate,
                discard_applied_behavior_placement_terrain_flattening
                    .in_set(FixedGameSet::Cleanup),
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
