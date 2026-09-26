use bevy::prelude::{Quat, Vec3};
use openzt2_game_data::AssetId;

use crate::plugins::world_spawn::persistent_id_types::PersistentId;

#[derive(Debug, Clone)]
pub(super) struct PhotoSnapshotRecord {
    pub persistent_identifier: PersistentId,
    pub album_persistent_identifier: PersistentId,
    pub album_order: u64,
    pub captured_simulation_tick: u64,
    pub camera_world_position: Vec3,
    pub camera_world_rotation: Quat,
    pub score_millipoints: i32,
    pub image_width_pixels: u32,
    pub image_height_pixels: u32,
    pub rgba8_pixel_bytes: Vec<u8>,
    pub challenge_photo_file_identifiers: Vec<AssetId>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct PhotoChallengeProgressSnapshotRecord {
    pub challenge_row_index: u32,
    pub completed_by_photo_persistent_identifier: Option<PersistentId>,
    pub rating_stars: u8,
}
