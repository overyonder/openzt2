use bevy::prelude::*;

use crate::assets::scene_prefab::ScenePrefabAsset;

/// A biome detail parented to its terrain chunk.
#[derive(Component, Debug, Clone)]
pub(super) struct TerrainDetail {
    pub(super) placement_key: u64,
    pub(super) prefab: Handle<ScenePrefabAsset>,
    pub(super) solid_m: f32,
    pub(super) fade_m: f32,
    pub(super) materialized_render_tree_root: Option<Entity>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(super) struct TerrainDetailProjectionState {
    pub(super) source_revision: u64,
    pub(super) retained_camera_focus: Vec2,
}

impl Default for TerrainDetailProjectionState {
    fn default() -> Self {
        Self {
            source_revision: u64::MAX,
            retained_camera_focus: Vec2::splat(f32::NAN),
        }
    }
}
