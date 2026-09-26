//! Instantiation and presentation policy for ordinary glTF model scenes.

pub(crate) mod authored_effect_technique_pass_submission_order;
pub(crate) mod authored_model_material_pass_projection;
mod gltf_model_scene_instantiation;
mod material_render_view_invalidation;
mod model_scene_visibility_and_shadow_projection;
mod prefab_ambient_light_projection;
mod prefab_fixed_function_world_lighting_projection;
mod prefab_model_billboard_orientation;
mod prefab_model_level_of_detail_projection;
mod prefab_model_tint_projection;
mod prefab_render_layer_camera_matching;

use bevy::{
    camera::visibility::VisibilitySystems,
    prelude::*,
    render::{extract_instances::ExtractInstancesPlugin, Render, RenderApp, RenderSystems},
    transform::TransformSystems,
};

use authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;

use authored_model_material_pass_projection::{
    invalidate_authored_model_material_pass_projection, project_authored_model_material_passes,
    project_effect_pass_material_instances_for_additional_render_views,
    update_per_draw_effect_transform_semantics,
};
use gltf_model_scene_instantiation::{
    complete_hidden_prefab_model_asset_release_after_world_instance_despawn,
    instantiate_visible_gltf_model_scenes, request_hidden_prefab_model_asset_release,
};
use model_scene_visibility_and_shadow_projection::project_model_scene_visibility_and_shadow_policy;
use prefab_ambient_light_projection::project_prefab_ambient_light_contributions;
use prefab_fixed_function_world_lighting_projection::{
    project_prefab_fixed_function_world_lighting_policies, update_fixed_function_world_light_buffer,
};
use prefab_model_billboard_orientation::orient_prefab_model_billboards_toward_active_camera_on_matching_render_layers;
use prefab_model_level_of_detail_projection::project_prefab_model_level_of_detail_visibility;
use prefab_model_tint_projection::project_prefab_model_tints_onto_standard_materials_and_effect_pass_vertex_colors;

pub struct ModelRenderPlugin;

impl Plugin for ModelRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractInstancesPlugin::<
            AuthoredEffectTechniquePassSubmissionOrder,
        >::extract_visible());
        app.add_systems(
            PostUpdate,
            (
                invalidate_authored_model_material_pass_projection,
                project_prefab_model_level_of_detail_visibility
                    .after(TransformSystems::Propagate)
                    .before(VisibilitySystems::VisibilityPropagate),
                instantiate_visible_gltf_model_scenes.after(VisibilitySystems::VisibilityPropagate),
                (
                    complete_hidden_prefab_model_asset_release_after_world_instance_despawn,
                    request_hidden_prefab_model_asset_release,
                )
                    .chain()
                    .after(instantiate_visible_gltf_model_scenes),
                project_model_scene_visibility_and_shadow_policy
                    .after(instantiate_visible_gltf_model_scenes),
                project_authored_model_material_passes
                    .after(project_model_scene_visibility_and_shadow_policy),
                update_per_draw_effect_transform_semantics
                    .after(TransformSystems::Propagate)
                    .after(VisibilitySystems::MarkNewlyHiddenEntitiesInvisible)
                    .after(project_effect_pass_material_instances_for_additional_render_views)
                    .after(project_prefab_fixed_function_world_lighting_policies),
                project_effect_pass_material_instances_for_additional_render_views
                    .after(project_authored_model_material_passes),
                project_prefab_ambient_light_contributions,
                update_fixed_function_world_light_buffer
                    .after(project_prefab_ambient_light_contributions)
                    .after(VisibilitySystems::CheckVisibility),
                project_prefab_fixed_function_world_lighting_policies
                    .after(project_authored_model_material_passes)
                    .after(update_fixed_function_world_light_buffer),
                project_prefab_model_tints_onto_standard_materials_and_effect_pass_vertex_colors
                    .after(project_model_scene_visibility_and_shadow_policy)
                    .after(project_prefab_fixed_function_world_lighting_policies),
                authored_effect_technique_pass_submission_order::project_authored_effect_opaque_base_pass_order
                    .after(project_authored_model_material_passes)
                    .after(project_prefab_fixed_function_world_lighting_policies),
                orient_prefab_model_billboards_toward_active_camera_on_matching_render_layers,
            ),
        );
        app.sub_app_mut(RenderApp)
            .add_systems(Render,
                authored_effect_technique_pass_submission_order::sort_authored_effect_invocations_before_bevy_mesh_batch_preparation
                    .after(RenderSystems::PhaseSort)
                    .before(RenderSystems::Prepare));
    }
}

#[derive(Component)]
pub(crate) struct ModelExpanded;
