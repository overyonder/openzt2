mod active_model_view_cameras;
mod configured_model_effect_materials;
mod environment_layer_rendering;
mod mirrored_model_primitive_winding;
pub(crate) mod model_projection_changes;
mod prefab_render_layer_camera_matching;
// Instantiation and presentation policy for ordinary glTF model scenes.

mod authored_effect_technique_pass_submission_order;
mod authored_model_material_pass_projection;
mod effect_environment_registers;
mod gltf_model_scene_instantiation;
mod model_scene_visibility_and_shadow_projection;
mod prefab_ambient_light_projection;
mod prefab_fixed_function_world_lighting_projection;
mod prefab_model_billboard_orientation;
mod prefab_model_level_of_detail_projection;
mod prefab_model_tint_projection;
pub(crate) mod world_entity_model_suppression;

use bevy::camera::visibility::VisibilitySystems;
use bevy::extract::extract_instances::ExtractInstancesPlugin;
use bevy::prelude::*;
use bevy::render::Render;
use bevy::render::RenderApp;
use bevy::render::RenderSystems;
use bevy::transform::TransformSystems;

use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use openzt2_world_state::model_render::ModelRenderPresentationSet;

use authored_model_material_pass_projection::{
    initialize_model_geometry_texture_coordinate_clocks, project_authored_model_material_passes,
};
use gltf_model_scene_instantiation::{
    bind_prefab_model_scene_ownership_on_ready, instantiate_visible_gltf_model_scenes,
    retire_hidden_prefab_model_assets,
};
use prefab_ambient_light_projection::project_prefab_ambient_light_contributions;
use prefab_fixed_function_world_lighting_projection::update_fixed_function_world_light_buffer;
use prefab_model_billboard_orientation::orient_prefab_model_billboards_toward_active_camera_on_matching_render_layers;
use prefab_model_level_of_detail_projection::project_prefab_model_level_of_detail_visibility;
use prefab_model_tint_projection::project_model_tints_onto_standard_materials;

pub struct ModelRenderPlugin;

impl Plugin for ModelRenderPlugin {
    fn build(&self, app: &mut App) {
        prefab_render_layer_camera_matching::register_inherited_render_layers(app);
        configured_model_effect_materials::register_configured_model_effect_materials(app);
        world_entity_model_suppression::register_inherited_model_render_suppression(app);
        model_scene_visibility_and_shadow_projection::register_inherited_model_draw_shadow_policy(
            app,
        );
        environment_layer_rendering::register_inherited_environment_render_layers(app);
        model_projection_changes::register(app);
        app.add_plugins(ExtractInstancesPlugin::<
            AuthoredEffectTechniquePassSubmissionOrder,
            RenderApp,
        >::extract_visible());
        app.add_observer(bind_prefab_model_scene_ownership_on_ready);
        app.add_plugins(ExtractInstancesPlugin::<
            environment_layer_rendering::EnvironmentPrimitiveLayer,
            RenderApp,
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
        app.configure_sets(
            PostUpdate,
            ModelRenderPresentationSet::LevelOfDetail
                .after(TransformSystems::Propagate)
                .before(VisibilitySystems::VisibilityPropagate),
        );
        app.configure_sets(
            PostUpdate,
            ModelRenderPresentationSet::VisibilityInputs
                .after(ModelRenderPresentationSet::SourceMaterials)
                .before(VisibilitySystems::CheckVisibility),
        );
        app.add_systems(PostUpdate,
            orient_prefab_model_billboards_toward_active_camera_on_matching_render_layers
                .after(bevy::app::AnimationSystems)
                .in_set(openzt2_world_state::world_spawn::prefab_authored_billboard_orientation_mode::PrefabBillboardProjection)
                .before(TransformSystems::Propagate));
        app.add_systems(
            PostUpdate,
            (
                project_prefab_ambient_light_contributions
                    .after(VisibilitySystems::VisibilityPropagate),
                update_fixed_function_world_light_buffer
                    .after(project_prefab_ambient_light_contributions)
                    .after(VisibilitySystems::MarkNewlyHiddenEntitiesInvisible),
                effect_environment_registers::write_effect_environment_registers
                    .after(TransformSystems::Propagate)
                    .after(VisibilitySystems::MarkNewlyHiddenEntitiesInvisible),
                crate::model_render::authored_effect_technique_pass_submission_order::project_authored_effect_opaque_base_pass_order
                    .after(ModelRenderPresentationSet::SourceMaterials),
            ),
        );
        app.add_systems(
            PostUpdate,
            (
                initialize_model_geometry_texture_coordinate_clocks.before(project_authored_model_material_passes),
                project_prefab_model_level_of_detail_visibility
                    .in_set(ModelRenderPresentationSet::LevelOfDetail),
                instantiate_visible_gltf_model_scenes
                    .after(VisibilitySystems::VisibilityPropagate)
                    .before(VisibilitySystems::CalculateBounds),
                retire_hidden_prefab_model_assets
                    .after(VisibilitySystems::VisibilityPropagate)
                    .after_ignore_deferred(instantiate_visible_gltf_model_scenes)
                    .before(VisibilitySystems::CheckVisibility),
                mirrored_model_primitive_winding::project_mirrored_and_tinted_model_primitive_meshes
                    .after(bevy::app::AnimationSystems)
                    .after(project_authored_model_material_passes)
                    .before(TransformSystems::Propagate)
                    .in_set(ModelRenderPresentationSet::SourceMaterials),
                project_authored_model_material_passes
                    .in_set(ModelRenderPresentationSet::SourceMaterials),
                project_model_tints_onto_standard_materials
                    .after(ModelRenderPresentationSet::SourceMaterials),
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
