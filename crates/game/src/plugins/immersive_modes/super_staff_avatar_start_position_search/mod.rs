//! Where the walking super-staff avatar appears when first-person mode opens.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::plugins::habitat::{
    habitat_membership_and_containment::locate_habitat_at_world_position,
    habitat_types::HabitatIndex,
};
use crate::plugins::terrain::terrain_chunk_types::{
    EditedTerrainSamples, TerrainChunk, TerrainIndex,
};
use crate::plugins::terrain::terrain_world_sampling::{sample_terrain, terrain_chunk_at};
use crate::plugins::topology::topology_graph_types::TopologyGrid;

/// The search advances along the overhead view's heading in placement-grid
/// cell steps and gives up after this many cells.
const AVATAR_START_SEARCH_STEP_M: f32 = 3.0;
const AVATAR_START_SEARCH_STEP_COUNT: u32 = 150;

#[derive(SystemParam)]
pub(super) struct SuperStaffAvatarStartSurface<'w, 's> {
    terrain_index: Res<'w, TerrainIndex>,
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
    terrain_chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
    habitat_index: Res<'w, HabitatIndex>,
    topology_grid: Option<Res<'w, TopologyGrid>>,
}

impl SuperStaffAvatarStartSurface<'_, '_> {
    fn terrain_height_at(&self, world_xz: Vec2) -> Option<f32> {
        let chunk_entity = terrain_chunk_at(&self.terrain_index, world_xz)?;
        let (chunk, edited) = self.terrain_chunks.get(chunk_entity).ok()?;
        let asset = self.terrain_assets.get(&chunk.asset)?;
        sample_terrain(chunk, asset, edited, world_xz).map(|terrain| terrain.height_m)
    }

    fn is_inside_habitat(&self, world_xz: Vec2) -> bool {
        self.topology_grid.as_deref().is_some_and(|topology_grid| {
            locate_habitat_at_world_position(&self.habitat_index, world_xz, *topology_grid)
                .is_some()
        })
    }

    /// Starts on the ground beneath the overhead camera. When that point is
    /// off the map or inside an enclosure, the search walks forward along the
    /// camera heading; if nothing qualifies, the camera position is used.
    pub(super) fn find_avatar_start_position(&self, camera_eye: Vec3, heading_yaw: f32) -> Vec3 {
        let step =
            (Quat::from_rotation_y(heading_yaw) * Vec3::NEG_Z).xz() * AVATAR_START_SEARCH_STEP_M;
        let mut candidate = camera_eye.xz();
        for _ in 0..AVATAR_START_SEARCH_STEP_COUNT {
            if !self.is_inside_habitat(candidate) {
                if let Some(height) = self.terrain_height_at(candidate) {
                    return Vec3::new(candidate.x, height, candidate.y);
                }
            }
            candidate += step;
        }
        Vec3::new(
            camera_eye.x,
            self.terrain_height_at(camera_eye.xz())
                .unwrap_or(camera_eye.y),
            camera_eye.z,
        )
    }
}
