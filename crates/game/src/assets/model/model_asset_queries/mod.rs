//! Borrowing mesh and aggregate-bounds queries for loaded glTF assets.

use bevy::{
    camera::primitives::MeshAabb,
    gltf::{Gltf, GltfMesh},
    prelude::*,
};

/// Visits the Bevy meshes referenced by an ordinary glTF model.
pub(crate) fn loaded_gltf_model_mesh_asset_handles<'a>(
    model: &'a Gltf,
    gltf_meshes: &'a Assets<GltfMesh>,
) -> impl Iterator<Item = &'a Handle<Mesh>> + 'a {
    model
        .meshes
        .iter()
        .filter_map(|handle| gltf_meshes.get(handle))
        .flat_map(|mesh| mesh.primitives.iter().map(|primitive| &primitive.mesh))
}

/// Computes the aggregate local bounds from Bevy's loaded mesh assets.
pub(crate) fn loaded_gltf_model_aggregate_local_bounds(
    model: &Gltf,
    gltf_meshes: &Assets<GltfMesh>,
    meshes: &Assets<Mesh>,
) -> Option<(Vec3, Vec3)> {
    loaded_gltf_model_mesh_asset_handles(model, gltf_meshes)
        .filter_map(|handle| meshes.get(handle)?.compute_aabb())
        .fold(None, |bounds, aabb| {
            let center = Vec3::from(aabb.center);
            let half = Vec3::from(aabb.half_extents);
            let minimum = center - half;
            let maximum = center + half;
            Some(bounds.map_or(
                (minimum, maximum),
                |(old_minimum, old_maximum): (Vec3, Vec3)| {
                    (old_minimum.min(minimum), old_maximum.max(maximum))
                },
            ))
        })
}
