mod environment_layer_rendering;
mod mirrored_model_primitive_winding;
mod prefab_render_layer_camera_matching;
// Instantiation and presentation policy for ordinary glTF model scenes.

mod authored_effect_technique_pass_submission_order;
mod authored_model_material_pass_projection;
mod gltf_model_scene_instantiation;
mod material_render_view_invalidation;
mod model_scene_visibility_and_shadow_projection;
mod prefab_ambient_light_projection;
mod prefab_fixed_function_world_lighting_projection;
mod prefab_model_billboard_orientation;
mod prefab_model_level_of_detail_projection;
pub mod prefab_model_tint_projection;
mod prefab_texture_replacement_projection;
mod world_entity_model_suppression;

use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;
use bevy::render::extract_instances::ExtractInstancesPlugin;
use bevy::render::Render;
use bevy::render::RenderApp;
use bevy::render::RenderSystems;
use bevy::transform::TransformSystems;

use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use openzt2_world_state::model_render::ModelRenderPresentationSet;

use authored_model_material_pass_projection::{
    invalidate_authored_model_material_pass_projection, project_authored_model_material_passes,
    project_effect_pass_material_instances_for_additional_render_views,
    update_per_draw_effect_transform_semantics,
};
use gltf_model_scene_instantiation::{
    complete_hidden_prefab_model_asset_release_after_world_instance_despawn,
    instantiate_visible_gltf_model_scenes, request_hidden_prefab_model_asset_release,
};
use model_scene_visibility_and_shadow_projection::{
    invalidate_model_scene_visibility_and_shadow_policy_on_world_instance_ready,
    project_model_scene_visibility_and_shadow_policy,
    project_model_scene_visibility_and_shadow_policy_onto_late_primitives,
};
use prefab_ambient_light_projection::project_prefab_ambient_light_contributions;
use prefab_fixed_function_world_lighting_projection::{
    project_prefab_fixed_function_world_lighting_policies, update_fixed_function_world_light_buffer,
};
use prefab_model_billboard_orientation::orient_prefab_model_billboards_toward_active_camera_on_matching_render_layers;
use prefab_model_level_of_detail_projection::project_prefab_model_level_of_detail_visibility;
use prefab_model_tint_projection::project_prefab_model_tints_onto_standard_materials_and_effect_pass_vertex_colors;
use prefab_texture_replacement_projection::project_prefab_texture_replacements_onto_effect_pass_materials;

pub struct ModelRenderPlugin;

impl Plugin for ModelRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractInstancesPlugin::<
            AuthoredEffectTechniquePassSubmissionOrder,
        >::extract_visible());
        app.add_observer(
            invalidate_model_scene_visibility_and_shadow_policy_on_world_instance_ready,
        );
        app.add_plugins(ExtractInstancesPlugin::<
            environment_layer_rendering::EnvironmentPrimitiveLayer,
        >::new());
        app.sub_app_mut(RenderApp)
            .init_resource::<environment_layer_rendering::PreparedEnvironmentLayers>()
            .add_systems(
                Render,
                environment_layer_rendering::restore_environment_phase_items_before_queueing
                    .before(RenderSystems::Queue),
            )
            .add_systems(
                Render,
                environment_layer_rendering::separate_prepared_environment_layers
                    .after(RenderSystems::Prepare)
                    .before(RenderSystems::Render),
            )
            .add_systems(
                bevy::core_pipeline::schedule::Core3d,
                environment_layer_rendering::render_environment_layers_before_world
                    .in_set(bevy::core_pipeline::schedule::Core3dSystems::MainPass)
                    .before(bevy::core_pipeline::core_3d::main_opaque_pass_3d),
            );
        app.add_systems(
            PostUpdate,
            environment_layer_rendering::project_environment_layers_to_model_primitives
                .after(ModelRenderPresentationSet::PassTransforms),
        );
        app.configure_sets(
            PostUpdate,
            (
                ModelRenderPresentationSet::LevelOfDetail,
                ModelRenderPresentationSet::MaterialProjection,
                ModelRenderPresentationSet::PassTransforms,
            )
                .chain()
                .after(TransformSystems::Propagate)
                .before(VisibilitySystems::VisibilityPropagate),
        );
        app.add_systems(PostUpdate,
            orient_prefab_model_billboards_toward_active_camera_on_matching_render_layers
                .after(bevy::app::AnimationSystems)
                .in_set(openzt2_world_state::world_spawn::prefab_authored_billboard_orientation_mode::PrefabBillboardProjection)
                .before(TransformSystems::Propagate));
        app.add_systems(
            PostUpdate,
            (
                invalidate_authored_model_material_pass_projection.before(project_authored_model_material_passes),
                project_prefab_model_level_of_detail_visibility
                    .in_set(ModelRenderPresentationSet::LevelOfDetail),
                instantiate_visible_gltf_model_scenes.after(VisibilitySystems::VisibilityPropagate),
                (
                    complete_hidden_prefab_model_asset_release_after_world_instance_despawn,
                    request_hidden_prefab_model_asset_release,
                )
                    .chain()
                    .after(instantiate_visible_gltf_model_scenes),
                project_model_scene_visibility_and_shadow_policy
                    .in_set(ModelRenderPresentationSet::MaterialProjection),
                project_model_scene_visibility_and_shadow_policy_onto_late_primitives
                    .after(project_model_scene_visibility_and_shadow_policy)
                    .before(project_authored_model_material_passes)
                    .in_set(ModelRenderPresentationSet::MaterialProjection),
                mirrored_model_primitive_winding::project_mirrored_model_primitive_winding
                    .in_set(ModelRenderPresentationSet::MaterialProjection)
                    .before(project_authored_model_material_passes)
                    .before(project_prefab_model_tints_onto_standard_materials_and_effect_pass_vertex_colors),
                project_authored_model_material_passes
                    .after(project_model_scene_visibility_and_shadow_policy)
                    .in_set(ModelRenderPresentationSet::MaterialProjection),
                update_per_draw_effect_transform_semantics
                    .after(TransformSystems::Propagate)
                    .after(VisibilitySystems::MarkNewlyHiddenEntitiesInvisible)
                    .after(project_effect_pass_material_instances_for_additional_render_views)
                    .after(project_prefab_fixed_function_world_lighting_policies),
                project_effect_pass_material_instances_for_additional_render_views
                    .after(project_authored_model_material_passes)
                    .in_set(ModelRenderPresentationSet::MaterialProjection),
                bevy::transform::systems::propagate_transforms_for::<Or<(
                    With<authored_model_material_pass_projection::AdditionalAuthoredModelMaterialPass>,
                    With<authored_model_material_pass_projection::EffectPassMaterialRenderViewProjection>,
                )>>
                    .in_set(ModelRenderPresentationSet::PassTransforms),
                crate::model_render::world_entity_model_suppression::project_world_entity_model_suppression
                    .after(project_effect_pass_material_instances_for_additional_render_views)
                    .in_set(ModelRenderPresentationSet::MaterialProjection),
                project_prefab_ambient_light_contributions
                    .after(crate::model_render::world_entity_model_suppression::project_world_entity_model_suppression),
                update_fixed_function_world_light_buffer
                    .after(project_prefab_ambient_light_contributions)
                    .after(VisibilitySystems::CheckVisibility),
                project_prefab_fixed_function_world_lighting_policies
                    .after(project_authored_model_material_passes)
                    .after(update_fixed_function_world_light_buffer),
                project_prefab_model_tints_onto_standard_materials_and_effect_pass_vertex_colors
                    .after(project_model_scene_visibility_and_shadow_policy)
                    .after(project_prefab_fixed_function_world_lighting_policies),
                crate::model_render::authored_effect_technique_pass_submission_order::project_authored_effect_opaque_base_pass_order
                    .after(project_authored_model_material_passes)
                    .after(project_prefab_fixed_function_world_lighting_policies),
                project_prefab_texture_replacements_onto_effect_pass_materials
                    .after(project_authored_model_material_passes)
                    .before(project_prefab_fixed_function_world_lighting_policies)
                    .in_set(ModelRenderPresentationSet::MaterialProjection),
            )
                // Material replacement must reach pipeline specialization in
                // the same frame as render extraction sees the new binding.
                .before(bevy::pbr::check_entities_needing_specialization::<StandardMaterial>)
                .before(bevy::pbr::check_entities_needing_specialization::<
                    openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial,
                >),
        );
        app.sub_app_mut(RenderApp)
            .add_systems(Render,
                crate::model_render::authored_effect_technique_pass_submission_order::sort_authored_effect_invocations_before_bevy_mesh_batch_preparation
                    .after(RenderSystems::PhaseSort)
                    .before(RenderSystems::Prepare));
    }
}
