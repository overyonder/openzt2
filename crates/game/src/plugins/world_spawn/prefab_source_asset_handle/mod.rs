use bevy::prelude::*;

use crate::assets::scene_prefab::ScenePrefabAsset;

/// The strong typed handle retained by one prefab instance root keeps its
/// scene-prefab asset alive without copying its source rows.
#[derive(Component, Debug, Clone)]
pub(crate) struct PrefabSourceAssetHandle(pub(crate) Handle<ScenePrefabAsset>);
