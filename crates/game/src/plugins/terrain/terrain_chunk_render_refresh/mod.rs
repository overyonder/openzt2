use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::terrain_change_tracking_types::{TerrainDirty, TerrainDirtyFlags};
use super::terrain_chunk_mesh_construction::construct_terrain_chunk_mesh;
use super::terrain_chunk_presentation_types::{TerrainRenderRevision, TerrainSurfaceImage};
use super::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk};
use super::terrain_surface_image_composition::compose_terrain_surface_image;

pub(super) fn refresh_changed_terrain_chunk_meshes_and_surface_images(
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    mut chunks: Query<(
        &TerrainChunk,
        &EditedTerrainSamples,
        &TerrainDirty,
        &Mesh3d,
        &TerrainSurfaceImage,
        &mut TerrainRenderRevision,
    )>,
) {
    let surface_targets = chunks
        .iter()
        .filter_map(|(chunk, _, dirty, _, _, applied)| {
            (applied.0 < dirty.revision && dirty.flags.contains(TerrainDirtyFlags::SURFACE))
                .then(|| (chunk.asset.clone(), chunk.coord))
        })
        .collect::<Vec<_>>();
    let edited_chunks = chunks
        .iter()
        .filter(|(chunk, _, _, _, _, _)| {
            surface_targets.iter().any(|(asset, coord)| {
                asset == &chunk.asset && (chunk.coord - *coord).abs().cmple(IVec2::ONE).all()
            })
        })
        .map(|(chunk, edited, _, _, _, _)| (chunk.asset.clone(), chunk.coord, edited.clone()))
        .collect::<Vec<_>>();
    for (chunk, edited, dirty, mesh_handle, surface_handle, mut applied) in &mut chunks {
        if applied.0 >= dirty.revision {
            continue;
        }
        let Some(asset) = terrain_assets.get(&chunk.asset) else {
            continue;
        };
        if dirty.flags.contains(TerrainDirtyFlags::HEIGHT) {
            let Some(rebuilt) = construct_terrain_chunk_mesh(chunk, asset, Some(edited)) else {
                continue;
            };
            let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) else {
                continue;
            };
            *mesh = rebuilt;
        }
        if dirty.flags.contains(TerrainDirtyFlags::SURFACE) {
            let Some(surface) =
                compose_terrain_surface_image(chunk, asset, &edited_chunks, &images)
            else {
                continue;
            };
            let Some(mut image) = images.get_mut(&surface_handle.0) else {
                continue;
            };
            *image = surface;
        }
        applied.0 = dirty.revision;
    }
}
