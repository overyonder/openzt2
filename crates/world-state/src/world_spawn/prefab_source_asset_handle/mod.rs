use bevy::prelude::*;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;

/// The strong typed handle retained by one prefab instance root keeps its
/// scene-prefab asset alive without copying its source rows.
#[derive(Component, Debug, Clone)]
pub struct PrefabSourceAssetHandle(pub Handle<ScenePrefabAsset>);
