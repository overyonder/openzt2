use bevy::prelude::*;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;

/// A biome detail parented to its terrain chunk.
#[derive(Component, Debug, Clone, PartialEq)]
pub(super) struct TerrainDetail {
    pub(super) placement_key: u64,
    pub(super) prefab: Handle<ScenePrefabAsset>,
    pub(super) solid_m: f32,
    pub(super) fade_m: f32,
}

#[derive(Component)]
pub(super) struct TerrainDetailRenderRoot;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct TerrainDetailProjectionState {
    pub(super) retained_camera_focus: Option<Vec2>,
}
