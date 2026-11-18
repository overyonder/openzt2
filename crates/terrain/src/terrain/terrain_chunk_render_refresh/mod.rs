use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use crate::terrain::terrain_change_tracking_types::TerrainDirtyFlags;
use crate::terrain::terrain_change_tracking_types::TerrainRenderPending;
use crate::terrain::terrain_chunk_mesh_construction::construct_terrain_chunk_mesh;
use crate::terrain::terrain_surface_image_composition::compose_terrain_surface_image;
use crate::terrain::terrain_surface_image_composition::recompose_terrain_surface_image_region;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainRenderChunk;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainSurfaceImage;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;

/// Rebuilds each changed chunk's mesh and surface image from the samples
/// changed since its last rebuild. The surface image reads heights and water
/// as well as ground paint, so it is recomposed only where those samples reach.
#[allow(clippy::type_complexity)]
pub(super) fn refresh_changed_terrain_chunk_meshes_and_surface_images(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    mut chunks: Query<(
        Entity,
        &TerrainChunk,
        Option<&EditedTerrainSamples>,
        &TerrainRenderPending,
        &Mesh3d,
        &TerrainSurfaceImage,
        &mut TerrainRenderChunk,
    )>,
    edited: Query<(&TerrainChunk, &EditedTerrainSamples)>,
) {
    if chunks.is_empty() {
        return;
    }
    let surface_flags =
        TerrainDirtyFlags::HEIGHT | TerrainDirtyFlags::SURFACE | TerrainDirtyFlags::WATER;
    let surface_targets = chunks
        .iter()
        .filter(|(_, _, _, render, ..)| render.0.flags.intersects(surface_flags))
        .map(|(_, chunk, ..)| (chunk.asset.clone(), chunk.coord))
        .collect::<Vec<_>>();
    let collect_span = info_span!("collect_neighbour_terrain_edits").entered();
    let edited_chunks = edited
        .iter()
        .filter(|(chunk, _)| {
            surface_targets.iter().any(|(asset, coord)| {
                asset == &chunk.asset && (chunk.coord - *coord).abs().cmple(IVec2::ONE).all()
            })
        })
        .map(|(chunk, edited)| (chunk.asset.clone(), chunk.coord, edited.clone()))
        .collect::<Vec<_>>();
    drop(collect_span);
    for (entity, chunk, edited, render, mesh_handle, surface_handle, mut rendered) in &mut chunks {
        let render = render.0;
        let Some(asset) = terrain_assets.get(&chunk.asset) else {
            continue;
        };
        if render.flags.contains(TerrainDirtyFlags::HEIGHT) {
            let _span = info_span!("construct_terrain_chunk_mesh").entered();
            let Some(rebuilt) = construct_terrain_chunk_mesh(chunk, asset, edited) else {
                continue;
            };
            let Some(mut mesh) = meshes.get_mut(&mesh_handle.0) else {
                continue;
            };
            *mesh = rebuilt;
        }
        if render.flags.intersects(surface_flags) {
            let _span = info_span!("compose_terrain_surface_image").entered();
            let surface = match images.get(&surface_handle.0) {
                Some(previous) => recompose_terrain_surface_image_region(
                    chunk,
                    asset,
                    &edited_chunks,
                    &images,
                    previous,
                    render.min,
                    render.max,
                ),
                None => compose_terrain_surface_image(chunk, asset, &edited_chunks, &images),
            };
            let Some(surface) = surface else {
                continue;
            };
            let Some(mut image) = images.get_mut(&surface_handle.0) else {
                continue;
            };
            *image = surface;
        }
        commands.entity(entity).remove::<TerrainRenderPending>();
        rendered.set_changed();
    }
}
