use bevy::asset::AssetId;
use bevy::mesh::Indices;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use bevy::transform::helper::TransformHelper;

use openzt2_world_state::model_render::ModelExpanded;

use super::authored_model_material_pass_projection::AdditionalAuthoredModelMaterialPass;

#[derive(Component)]
pub(super) struct ModelPrimitiveWindingProjected;

#[expect(clippy::needless_pass_by_value)]
pub(super) fn project_mirrored_model_primitive_winding(
    mut commands: Commands,
    mut changes: super::model_projection_changes::ModelProjectionChanges,
    roots: Query<(), With<ModelExpanded>>,
    parents: Query<&ChildOf>,
    changed_meshes: Query<Entity, (Changed<Mesh3d>, Without<ModelPrimitiveWindingProjected>)>,
    mut removed_markers: RemovedComponents<ModelPrimitiveWindingProjected>,
    primitives: Query<
        &Mesh3d,
        (
            Without<ModelPrimitiveWindingProjected>,
            Without<AdditionalAuthoredModelMaterialPass>,
        ),
    >,
    transforms: TransformHelper,
    mut meshes: ResMut<Assets<Mesh>>,
    mut reflected_meshes: Local<HashMap<AssetId<Mesh>, AssetId<Mesh>>>,
    mut asset_events: MessageReader<AssetEvent<Mesh>>,
) {
    for event in asset_events.read() {
        if let AssetEvent::Removed { id } = event {
            reflected_meshes.remove(id);
        }
    }
    changes.collect(
        std::iter::empty(),
        changed_meshes.iter().chain(removed_markers.read()),
    );
    let mut missing = Vec::new();
    for entity in changes.candidates() {
        let Ok(source) = primitives.get(entity) else {
            continue;
        };
        if !std::iter::once(entity)
            .chain(parents.iter_ancestors(entity))
            .any(|node| roots.contains(node))
        {
            continue;
        }
        let Ok(transform) = transforms.compute_global_transform(entity) else {
            missing.push(entity);
            continue;
        };
        let determinant = transform.affine().matrix3.determinant();
        if !determinant.is_finite() || determinant == 0.0 {
            missing.push(entity);
            continue;
        }
        if determinant < 0.0 {
            let source = &source.0;
            let cached = reflected_meshes
                .get(&source.id())
                .copied()
                .and_then(|asset| meshes.get_strong_handle(asset));
            let handle = if let Some(handle) = cached {
                handle
            } else {
                let Some(mut mesh) = meshes.get(source).cloned() else {
                    missing.push(entity);
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
                reflected_meshes.insert(source.id(), handle.id());
                handle
            };
            commands.entity(entity).insert(Mesh3d(handle));
        }
        commands
            .entity(entity)
            .insert(ModelPrimitiveWindingProjected);
    }
    changes.retry_missing(&mut missing);
}
