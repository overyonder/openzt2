use bevy::prelude::*;

use crate::terrain::terrain_change_tracking_types::TerrainDirtyFlags;
use crate::terrain::terrain_change_tracking_types::TerrainDirtyRegion;
use crate::terrain::terrain_sample_change_calculations::merge_terrain_dirty_sample_rectangles;

#[test]
fn dirty_terrain_sample_rectangles_coalesce() {
    let merged_dirty_rectangle = merge_terrain_dirty_sample_rectangles(
        TerrainDirtyRegion {
            min: UVec2::new(4, 6),
            max: UVec2::new(8, 9),
            flags: TerrainDirtyFlags::HEIGHT,
        },
        TerrainDirtyRegion {
            min: UVec2::new(2, 7),
            max: UVec2::new(6, 12),
            flags: TerrainDirtyFlags::WATER,
        },
    );
    assert_eq!(
        merged_dirty_rectangle,
        TerrainDirtyRegion {
            min: UVec2::new(2, 6),
            max: UVec2::new(8, 12),
            flags: TerrainDirtyFlags::HEIGHT | TerrainDirtyFlags::WATER,
        },
    );
}
