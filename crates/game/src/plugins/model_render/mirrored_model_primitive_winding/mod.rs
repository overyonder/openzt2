use bevy::{
    gltf::GltfMaterialName,
    mesh::{Indices, PrimitiveTopology},
    platform::collections::{HashMap, HashSet},
    prelude::*,
};

/// Reverses the triangle winding of model primitives placed under an odd
/// number of reflecting transforms, such as mirrored fence curves. Bevy keeps
/// the authored cull face under a negative determinant, so without this the
/// mirrored mesh shows only its back faces.
pub(super) fn reverse_triangle_winding_of_mirrored_model_primitives(
    mut primitives: Query<(Entity, &mut Mesh3d), (Added<Mesh3d>, With<GltfMaterialName>)>,
    hierarchy: Query<(&Transform, Option<&ChildOf>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    // ponytail: one reversed copy per source mesh, kept for the session.
    mut reversed: Local<HashMap<AssetId<Mesh>, Handle<Mesh>>>,
    mut reversed_ids: Local<HashSet<AssetId<Mesh>>>,
) {
    for (entity, mut mesh) in &mut primitives {
        if reversed_ids.contains(&mesh.id()) || !is_mirrored(entity, &hierarchy) {
            continue;
        }
        let handle = if let Some(handle) = reversed.get(&mesh.id()) {
            handle.clone()
        } else {
            let Some(source) = meshes.get(&mesh.0) else {
                continue;
            };
            let Some(flipped) = with_reversed_triangle_winding(source) else {
                continue;
            };
            let handle = meshes.add(flipped);
            reversed.insert(mesh.id(), handle.clone());
            reversed_ids.insert(handle.id());
            handle
        };
        mesh.0 = handle;
    }
}

fn is_mirrored(mut entity: Entity, hierarchy: &Query<(&Transform, Option<&ChildOf>)>) -> bool {
    let mut mirrored = false;
    while let Ok((transform, parent)) = hierarchy.get(entity) {
        mirrored ^= transform.scale.x * transform.scale.y * transform.scale.z < 0.0;
        let Some(parent) = parent else { break };
        entity = parent.parent();
    }
    mirrored
}

fn with_reversed_triangle_winding(source: &Mesh) -> Option<Mesh> {
    if source.primitive_topology() != PrimitiveTopology::TriangleList {
        return None;
    }
    let mut mesh = source.clone();
    match mesh.indices_mut() {
        Some(Indices::U16(indices)) => indices
            .as_chunks_mut::<3>()
            .0
            .iter_mut()
            .for_each(|t| t.swap(1, 2)),
        Some(Indices::U32(indices)) => indices
            .as_chunks_mut::<3>()
            .0
            .iter_mut()
            .for_each(|t| t.swap(1, 2)),
        None => {
            let count = u32::try_from(mesh.count_vertices()).ok()?;
            let mut indices = (0..count).collect::<Vec<_>>();
            indices
                .as_chunks_mut::<3>()
                .0
                .iter_mut()
                .for_each(|t| t.swap(1, 2));
            mesh.insert_indices(Indices::U32(indices));
        }
    }
    Some(mesh)
}

#[cfg(test)]
mod tests {
    use bevy::asset::RenderAssetUsages;

    use super::*;

    #[test]
    fn reverses_each_indexed_triangle() {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all());
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; 4]);
        mesh.insert_indices(Indices::U16(vec![0, 1, 2, 2, 1, 3]));
        let reversed = with_reversed_triangle_winding(&mesh).unwrap();
        assert!(matches!(
            reversed.indices(),
            Some(Indices::U16(indices)) if indices == &[0, 2, 1, 2, 3, 1]
        ));
    }
}
