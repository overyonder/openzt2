use bevy::asset::AssetId;
use bevy::mesh::Indices;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::transform::helper::TransformHelper;

use openzt2_world_state::model_render::ModelExpanded;

use super::authored_model_material_pass_projection::{
    AdditionalAuthoredModelMaterialPass, EffectPassMaterialRenderViewProjection,
};

#[derive(Component)]
struct ModelPrimitiveWindingProjected;

pub(super) fn project_mirrored_model_primitive_winding(
    mut commands: Commands,
    roots: Query<Entity, With<ModelExpanded>>,
    children: Query<&Children>,
    primitives: Query<
        &Mesh3d,
        (
            Without<ModelPrimitiveWindingProjected>,
            Without<AdditionalAuthoredModelMaterialPass>,
            Without<EffectPassMaterialRenderViewProjection>,
        ),
    >,
    transforms: TransformHelper,
    mut meshes: ResMut<Assets<Mesh>>,
    mut reflected_meshes: Local<HashMap<AssetId<Mesh>, AssetId<Mesh>>>,
) {
    reflected_meshes.retain(|source, reflected| {
        meshes.contains(*source) && meshes.contains(*reflected)
    });
    for root in &roots {
        for entity in std::iter::once(root).chain(children.iter_descendants(root)) {
            let Ok(source) = primitives.get(entity) else {
                continue;
            };
            let Ok(transform) = transforms.compute_global_transform(entity) else {
                continue;
            };
            let determinant = transform.affine().matrix3.determinant();
            if !determinant.is_finite() || determinant == 0.0 {
                continue;
            }
            if determinant < 0.0 {
                let cached = reflected_meshes
                    .get(&source.0.id())
                    .copied()
                    .and_then(|asset| meshes.get_strong_handle(asset));
                let reflected = if let Some(handle) = cached {
                    handle
                } else {
                    let Some(mut mesh) = meshes.get(&source.0).cloned() else {
                        continue;
                    };
                    if mesh.primitive_topology() == PrimitiveTopology::TriangleList
                        && mesh.indices().is_none()
                    {
                        let Ok(vertex_count) = u32::try_from(mesh.count_vertices()) else {
                            continue;
                        };
                        mesh.insert_indices(Indices::U32((0..vertex_count).collect()));
                    }
                    if let Err(error) = mesh.invert_winding() {
                        warn!(?entity, ?error, "model primitive winding projection failed");
                        continue;
                    }
                    let handle = meshes.add(mesh);
                    reflected_meshes.insert(source.0.id(), handle.id());
                    handle
                };
                commands.entity(entity).insert(Mesh3d(reflected));
            }
            commands.entity(entity).insert(ModelPrimitiveWindingProjected);
        }
    }
}
