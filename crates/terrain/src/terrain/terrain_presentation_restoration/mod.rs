use bevy::prelude::*;
use openzt2_zoo_state::terrain::{
    terrain_chunk_types::TerrainChunk, terrain_collision_rebuilding_types::TerrainCollisionPending,
    terrain_edit_types::TerrainPresentationRestored,
};

use super::{
    terrain_change_tracking_types::{TerrainDirty, TerrainDirtyFlags, TerrainDirtyRegion},
    terrain_sample_change_calculations::queue_terrain_render_refresh,
};

pub(super) fn mark_restored_terrain_chunks_for_presentation_rebuilding(
    mut commands: Commands,
    mut restored: MessageReader<TerrainPresentationRestored>,
    chunks: Query<&TerrainChunk>,
) {
    for request in restored.read() {
        let Ok(chunk) = chunks.get(request.chunk) else {
            continue;
        };
        let Some(last_sample) = chunk.side.checked_sub(1).filter(|last| *last > 0) else {
            continue;
        };
        let min = UVec2::ZERO;
        let max = UVec2::splat(u32::from(last_sample));
        let flags = TerrainDirtyFlags::HEIGHT | TerrainDirtyFlags::SURFACE;
        queue_terrain_render_refresh(
            &mut commands,
            request.chunk,
            TerrainDirtyRegion { min, max, flags },
        );
        commands
            .entity(request.chunk)
            .insert(TerrainDirty)
            .insert_if_new(TerrainCollisionPending);
    }
}
