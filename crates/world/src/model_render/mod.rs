pub mod active_model_view_cameras;
mod configured_model_effect_materials;
mod environment_layer_rendering;
mod mirrored_model_primitive_winding;
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
pub(crate) mod prefab_render_lod_tier_projection;
pub(crate) mod scoped_prefab_lighting;
pub(crate) mod world_entity_model_suppression;
mod world_light_inputs;

use bevy::camera::visibility::VisibilitySystems;
use bevy::core_pipeline::core_3d::Transparent3d;
use bevy::ecs::schedule::ScheduleCleanupPolicy;
use bevy::extract::extract_instances::ExtractInstancesPlugin;
use bevy::prelude::*;
use bevy::render::render_phase::sort_phase_system;
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
    instantiate_visible_gltf_model_scenes, retire_hidden_prefab_model_assets,
    retire_removed_prefab_scene_root,
};
use prefab_ambient_light_projection::project_prefab_ambient_light_contributions;
use prefab_fixed_function_world_lighting_projection::project_fixed_function_world_light_registers;
use prefab_model_billboard_orientation::orient_prefab_model_billboards_toward_active_camera_on_matching_render_layers;
use prefab_model_level_of_detail_projection::project_prefab_model_level_of_detail_visibility;
use prefab_model_tint_projection::project_model_tints_onto_standard_materials;

pub struct ModelRenderPlugin;

impl Plugin for ModelRenderPlugin {
    fn build(&self, app: &mut App) {
        prefab_render_layer_camera_matching::register_inherited_render_layers(app);
        prefab_model_level_of_detail_projection::register_prefab_lod_camera_visibility(app);
        configured_model_effect_materials::register_configured_model_effect_materials(app);
        world_entity_model_suppression::register_inherited_model_render_suppression(app);
        model_scene_visibility_and_shadow_projection::register_inherited_model_draw_shadow_policy(
            app,
        );
        environment_layer_rendering::register_inherited_environment_render_layers(app);
        mirrored_model_primitive_winding::register_inherited_primitive_tint(app);
        app.add_plugins(ExtractInstancesPlugin::<
            AuthoredEffectTechniquePassSubmissionOrder,
            RenderApp,
        >::extract_visible());
        app.add_observer(retire_removed_prefab_scene_root)
            .add_systems(
                SpawnScene,
                gltf_model_scene_instantiation::detach_attachments_from_reloading_scenes
                    .before(bevy::app::SceneSpawnerSystems::WorldInstanceSpawn)
                    .run_if(on_message::<AssetEvent<WorldAsset>>),
            );
        app.world_mut().spawn((
            effect_environment_registers::ProgrammableEnvironmentRegisters::default(),
            world_light_inputs::LinearWorldAmbient::default(),
            prefab_fixed_function_world_lighting_projection::FixedFunctionWorldLightRegisters::default(),
        ));
        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .remove_systems_in_set(
                Render,
                sort_phase_system::<Transparent3d>,
                ScheduleCleanupPolicy::RemoveSystemsOnly,
            )
            .expect("the Core3d transparent sort system is registered");
        render_app.add_systems(
            Render,
            authored_effect_technique_pass_submission_order::sort_transparent_effect_phase
                .in_set(RenderSystems::PhaseSort),
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
                scoped_prefab_lighting::project_scoped_prefab_lighting
                    .after(VisibilitySystems::VisibilityPropagate)
                    .before(configured_model_effect_materials::project_configured_model_effect_materials),
                world_light_inputs::publish_linear_world_ambient
                    .after(project_prefab_ambient_light_contributions),
                world_light_inputs::publish_directional_world_light_inputs
                    .after(TransformSystems::Propagate),
                project_fixed_function_world_light_registers
                    .after(world_light_inputs::publish_linear_world_ambient)
                    .after(world_light_inputs::publish_directional_world_light_inputs)
                    .after(VisibilitySystems::MarkNewlyHiddenEntitiesInvisible),
                effect_environment_registers::project_effect_environment_registers
                    .after(world_light_inputs::publish_linear_world_ambient)
                    .after(world_light_inputs::publish_directional_world_light_inputs)
                    .after(VisibilitySystems::MarkNewlyHiddenEntitiesInvisible),
                effect_environment_registers::upload_programmable_environment_registers
                    .after(effect_environment_registers::project_effect_environment_registers),
                prefab_fixed_function_world_lighting_projection::upload_fixed_function_world_light_registers
                    .after(project_fixed_function_world_light_registers),
                crate::model_render::authored_effect_technique_pass_submission_order::project_authored_effect_opaque_base_pass_order
                    .after(ModelRenderPresentationSet::SourceMaterials),
            ),
        );
        app.add_systems(
            PostUpdate,
            (
                initialize_model_geometry_texture_coordinate_clocks.before(
                    openzt2_game_assets::material::runtime::texture_coordinate_animation::TextureCoordinateAnimationSet,
                ),
                prefab_render_lod_tier_projection::project_prefab_render_lod_tier
                    .in_set(ModelRenderPresentationSet::LevelOfDetail),
                project_prefab_model_level_of_detail_visibility
                    .after(prefab_render_lod_tier_projection::project_prefab_render_lod_tier)
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
                    .after(TransformSystems::Propagate)
                    .before(VisibilitySystems::CalculateBounds)
                    .in_set(ModelRenderPresentationSet::SourceMaterials),
                project_authored_model_material_passes
                    .in_set(ModelRenderPresentationSet::SourceMaterials),
                authored_model_material_pass_projection::synchronize_additional_pass_geometry
                    .after(ModelRenderPresentationSet::SourceMaterials)
                    .after(ModelRenderPresentationSet::VisibilityInputs)
                    .before(VisibilitySystems::CalculateBounds),
                project_model_tints_onto_standard_materials
                    .after(ModelRenderPresentationSet::SourceMaterials)
                    .after(bevy::app::PropagateSet::<crate::world_spawn::prefab_world_instance_spawning::morph_fade_projection::PrefabMorphOpacity>::default())
                    .after(bevy::app::PropagateSet::<crate::world_spawn::prefab_world_instance_spawning::morph_fade_projection::NamedPrefabOpacity>::default()),
            )
                // Material replacement must reach pipeline specialization in
                // the same frame as render extraction sees the new binding.
                .before(bevy::pbr::check_entities_needing_specialization::<StandardMaterial>)
                .before(bevy::pbr::check_entities_needing_specialization::<
                    openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial,
                >),
        );
    }
}
