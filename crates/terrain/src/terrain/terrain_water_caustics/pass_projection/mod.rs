use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::{
    AuthoredEffectTechniquePassSubmissionOrder, AuthoredEffectTransparentRenderStage,
};
use openzt2_zoo_state::terrain::terrain_water_caustic_types::TerrainWaterCausticPass;

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_camera_specific_caustic_ground_passes(
    commands: &mut Commands,
    chunk_entity: Entity,
    water_owner: Entity,
    mesh: Handle<Mesh>,
    templates: &[EffectPassMaterial],
    render_views: &[(Entity, usize)],
    materials: &mut Assets<EffectPassMaterial>,
    buffers: &mut Assets<ShaderBuffer>,
) {
    for &(render_view, layer) in render_views {
        let mut invocation = None;
        for (index, template) in templates.iter().enumerate() {
            let mut pass = template.clone();
            pass.recreate_uniform_buffers_for_runtime_material(buffers);
            let entity = commands
                .spawn((
                    TerrainWaterCausticPass {
                        water_owner,
                        render_view,
                    },
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(materials.add(pass)),
                    Transform::IDENTITY,
                    Visibility::Hidden,
                    RenderLayers::layer(layer),
                    ChildOf(chunk_entity),
                ))
                .id();
            let invocation = *invocation.get_or_insert(entity);
            commands.entity(entity).insert(
                AuthoredEffectTechniquePassSubmissionOrder::new(invocation, index)
                    .expect("the caustic authored pass index fits u16")
                    .with_render_stage(AuthoredEffectTransparentRenderStage::TerrainCaustics),
            );
        }
    }
}
