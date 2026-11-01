use bevy::prelude::*;
use openzt2_zoo_state::terrain::{
    terrain_chunk_types::TerrainChunk, terrain_collision_rebuilding_types::TerrainCollisionPending,
    terrain_edit_types::TerrainPresentationRestored,
};

use super::{
    terrain_change_tracking_types::{TerrainDirty, TerrainDirtyFlags},
    terrain_sample_change_calculations::merge_terrain_dirty_sample_rectangle_and_advance_revision,
};

pub(super) fn mark_restored_terrain_chunks_for_presentation_rebuilding(
    mut commands: Commands,
    mut restored: MessageReader<TerrainPresentationRestored>,
    mut chunks: Query<(&TerrainChunk, Option<&mut TerrainDirty>)>,
) {
    for request in restored.read() {
        let Ok((chunk, dirty)) = chunks.get_mut(request.chunk) else {
            continue;
        };
        let Some(last_sample) = chunk.side.checked_sub(1).filter(|last| *last > 0) else {
            continue;
        };
        let min = UVec2::ZERO;
        let max = UVec2::splat(u32::from(last_sample));
        let flags =
            TerrainDirtyFlags::HEIGHT | TerrainDirtyFlags::SURFACE | TerrainDirtyFlags::COLLISION;
        if let Some(mut dirty) = dirty {
            *dirty = merge_terrain_dirty_sample_rectangle_and_advance_revision(
                Some(*dirty),
                min,
                max,
                flags,
            );
        } else {
            commands.entity(request.chunk).insert(
                merge_terrain_dirty_sample_rectangle_and_advance_revision(None, min, max, flags),
            );
        }
        commands
            .entity(request.chunk)
            .insert(TerrainCollisionPending);
    }
}
