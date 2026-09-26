use bevy::prelude::*;

use super::{
    terrain_change_tracking_types::{TerrainDirty, TerrainDirtyFlags},
    terrain_sample_change_calculations::merge_terrain_dirty_sample_rectangle_and_advance_revision,
};

#[test]
fn dirty_terrain_sample_rectangles_coalesce_and_revision_advances_once() {
    let first_dirty_rectangle = merge_terrain_dirty_sample_rectangle_and_advance_revision(
        None,
        UVec2::new(4, 6),
        UVec2::new(8, 9),
        TerrainDirtyFlags::HEIGHT,
    );
    let merged_dirty_rectangle = merge_terrain_dirty_sample_rectangle_and_advance_revision(
        Some(first_dirty_rectangle),
        UVec2::new(2, 7),
        UVec2::new(6, 12),
        TerrainDirtyFlags::WATER,
    );
    assert_eq!(
        merged_dirty_rectangle,
        TerrainDirty {
            min: UVec2::new(2, 6),
            max: UVec2::new(8, 12),
            flags: TerrainDirtyFlags::HEIGHT | TerrainDirtyFlags::WATER,
            revision: 2,
        }
    );
}
