use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use crate::terrain::terrain_change_tracking_types::TerrainDirtyFlags;
use crate::terrain::terrain_change_tracking_types::TerrainRenderPending;
use crate::terrain::terrain_change_tracking_types::TerrainSurfaceBasePending;
use crate::terrain::terrain_chunk_mesh_construction::construct_terrain_chunk_mesh;
use crate::terrain::terrain_sample_change_calculations::merge_terrain_dirty_sample_rectangles;
use openzt2_zoo_state::terrain::terrain_chunk_presentation_types::TerrainRenderChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;

/// Rebuilds each changed chunk's mesh and surface image from the samples
/// changed since its last rebuild. The surface image reads heights and water
/// as well as ground paint, so it is recomposed only where those samples reach.
#[allow(clippy::type_complexity)]
pub(super) fn refresh_changed_terrain_chunk_meshes_and_surface_images(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    mut chunks: Query<(
        Entity,
        &TerrainChunk,
        Option<&EditedTerrainSamples>,
        &TerrainRenderPending,
        &Mesh3d,
        Option<&mut TerrainSurfaceBasePending>,
        &mut TerrainRenderChunk,
    )>,
) {
    let surface_flags =
        TerrainDirtyFlags::HEIGHT | TerrainDirtyFlags::SURFACE | TerrainDirtyFlags::WATER;
    for (entity, chunk, edited, render, mesh_handle, surface_pending, mut rendered) in &mut chunks {
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
            if let Some(mut pending) = surface_pending {
                let merged = merge_terrain_dirty_sample_rectangles(pending.0, render);
                pending.set_if_neq(TerrainSurfaceBasePending(merged));
            } else {
                commands
                    .entity(entity)
                    .insert(TerrainSurfaceBasePending(render));
            }
        }
        commands.entity(entity).remove::<TerrainRenderPending>();
        rendered.set_changed();
    }
}
