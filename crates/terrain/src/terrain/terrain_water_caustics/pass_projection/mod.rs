use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::render::storage::ShaderBuffer;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_world_state::model_render::authored_effect_technique_pass_submission_order::{
    AuthoredEffectTechniquePassSubmissionOrder, AuthoredEffectTransparentRenderStage,
};
use openzt2_zoo_state::terrain::terrain_water_caustic_types::TerrainWaterCausticPass;

#[derive(Component)]
#[relationship(relationship_target = GroundCausticRegionDraws)]
pub(super) struct GroundCausticRegion(Entity);

#[derive(Component)]
#[relationship_target(relationship = GroundCausticRegion, linked_spawn)]
pub(super) struct GroundCausticRegionDraws(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = GroundCausticViewDraws)]
pub(super) struct GroundCausticView(Entity);

#[derive(Component)]
#[relationship_target(relationship = GroundCausticView, linked_spawn)]
pub(super) struct GroundCausticViewDraws(Vec<Entity>);

#[derive(Component)]
pub(in crate::terrain) struct GroundCausticPassIndex(pub(super) usize);

#[allow(clippy::too_many_arguments)]
pub(super) fn reconcile_camera_specific_caustic_ground_passes(
    commands: &mut Commands,
    chunk: Entity,
    water_owner: Entity,
    mesh: Handle<Mesh>,
    templates: &[EffectPassMaterial],
    render_views: &[(Entity, usize)],
    materials: &mut Assets<EffectPassMaterial>,
    buffers: &mut Assets<ShaderBuffer>,
    existing: &mut bevy::platform::collections::HashMap<(Entity, Entity, usize), Entity>,
    draws: &mut Query<(
        &TerrainWaterCausticPass,
        &GroundCausticPassIndex,
        &mut Mesh3d,
        &mut MeshMaterial3d<EffectPassMaterial>,
        &mut RenderLayers,
    )>,
) {
    for &(view, layer) in render_views {
        let mut invocation = None;
        for (index, template) in templates.iter().enumerate() {
            let mut pass = template.clone();
            pass.retain_authored_draw_order();
            pass.recreate_uniform_buffers_for_runtime_material(buffers);
            let material = materials.add(pass);
            let entity = if let Some(entity) = existing.remove(&(water_owner, view, index)) {
                let (_, _, mut geometry, mut current, mut layers) =
                    draws.get_mut(entity).expect("indexed ground caustic draw");
                geometry.0 = mesh.clone();
                current.0 = material;
                layers.set_if_neq(RenderLayers::layer(layer));
                entity
            } else {
                commands
                    .spawn((
                        TerrainWaterCausticPass {
                            water_owner,
                            render_view: view,
                        },
                        GroundCausticRegion(water_owner),
                        GroundCausticView(view),
                        GroundCausticPassIndex(index),
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material),
                        Transform::IDENTITY,
                        Visibility::Hidden,
                        RenderLayers::layer(layer),
                        ChildOf(chunk),
                    ))
                    .id()
            };
            let invocation = *invocation.get_or_insert(entity);
            commands.entity(entity).insert(
                AuthoredEffectTechniquePassSubmissionOrder::new(invocation, index)
                    .expect("the caustic authored pass index fits u16")
                    .with_render_stage(AuthoredEffectTransparentRenderStage::TerrainCaustics),
            );
        }
    }
}
