//! Where the walking super-staff avatar appears when first-person mode opens.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::plugins::habitat::{
    habitat_membership_and_containment::locate_habitat_at_world_position,
    habitat_types::HabitatIndex,
};
use crate::plugins::terrain::terrain_chunk_types::{
    EditedTerrainSamples, TerrainChunk, TerrainIndex,
};
use crate::plugins::terrain::terrain_world_sampling::{sample_terrain, terrain_chunk_at};
use crate::plugins::topology::topology_graph_types::TopologyGrid;
use crate::plugins::world_spawn::world_terrain_hydration::WorldTerrainHorizontalBounds;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

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
    /// inside an enclosure, the search walks forward along the camera heading.
    /// Entry and the retained fallback stay within the same world edges as movement.
    pub(super) fn find_avatar_start_position(
        &self,
        camera_eye: Vec3,
        heading_yaw: f32,
        bounds: &WorldTerrainHorizontalBounds,
    ) -> Vec3 {
        let step =
            (Quat::from_rotation_y(heading_yaw) * Vec3::NEG_Z).xz() * AVATAR_START_SEARCH_STEP_M;
        let start = camera_eye.xz().clamp(bounds.min, bounds.max);
        let mut candidate = start;
        for _ in 0..AVATAR_START_SEARCH_STEP_COUNT {
            if !self.is_inside_habitat(candidate) {
                if let Some(height) = self.terrain_height_at(candidate) {
                    return Vec3::new(candidate.x, height, candidate.y);
                }
            }
            candidate = (candidate + step).clamp(bounds.min, bounds.max);
        }
        Vec3::new(
            start.x,
            self.terrain_height_at(start).unwrap_or(camera_eye.y),
            start.y,
        )
    }
}
