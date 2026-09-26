use bevy::{gltf::Gltf, prelude::Assets};
use openzt2_game_data::{scene_prefab::PrefabColliderSource, AssetId};

use crate::assets::scene_prefab::ScenePrefabAsset;

pub(crate) fn first_missing_prefab_collider_model_asset_id(
    prefab: &ScenePrefabAsset,
    models: &Assets<Gltf>,
) -> Option<AssetId> {
    prefab
        .canonical_scene_prefab_document()
        .entities
        .iter()
        .flat_map(|entity| &entity.colliders)
        .filter_map(|collider| match &collider.source {
            PrefabColliderSource::Model { model, .. } => Some(AssetId(model.0)),
            PrefabColliderSource::Box { .. } | PrefabColliderSource::Capsule { .. } => None,
        })
        .find(|model| {
            prefab
                .loaded_model_asset_handle(*model)
                .is_none_or(|handle| models.get(handle).is_none())
        })
}
