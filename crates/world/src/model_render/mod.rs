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
        app.add_systems(
            PostUpdate,
            (
                invalidate_authored_model_material_pass_projection.before(project_authored_model_material_passes),
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
                    .after(request_hidden_prefab_model_asset_release)
                    .before(VisibilitySystems::CheckVisibility),
                project_model_scene_visibility_and_shadow_policy_onto_late_primitives
                    .after(project_model_scene_visibility_and_shadow_policy)
                    .before(project_authored_model_material_passes),
                project_authored_model_material_passes
                    .after(project_model_scene_visibility_and_shadow_policy),
                update_per_draw_effect_transform_semantics
                    .after(TransformSystems::Propagate)
                    .after(VisibilitySystems::MarkNewlyHiddenEntitiesInvisible)
                    .after(project_effect_pass_material_instances_for_additional_render_views)
                    .after(project_prefab_fixed_function_world_lighting_policies),
                project_effect_pass_material_instances_for_additional_render_views
                    .after(project_authored_model_material_passes),
                crate::model_render::world_entity_model_suppression::project_world_entity_model_suppression
                    .after(project_effect_pass_material_instances_for_additional_render_views)
                    .before(VisibilitySystems::CheckVisibility),
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
                orient_prefab_model_billboards_toward_active_camera_on_matching_render_layers,
                project_prefab_texture_replacements_onto_effect_pass_materials
                    .after(project_authored_model_material_passes)
                    .before(project_prefab_fixed_function_world_lighting_policies),
            ),
        );
        app.sub_app_mut(RenderApp)
            .add_systems(Render,
                crate::model_render::authored_effect_technique_pass_submission_order::sort_authored_effect_invocations_before_bevy_mesh_batch_preparation
                    .after(RenderSystems::PhaseSort)
                    .before(RenderSystems::Prepare));
    }
}
