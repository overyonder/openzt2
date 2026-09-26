use openzt2_game_data::{scene_prefab::ScenePrefabRenderableVisibilityFlags, AssetId};

use crate::assets::scene_prefab::ScenePrefabAsset;

use super::prefab_presentation_types::{
    PrefabMaterialOverride, PrefabMaterialOverrides, PrefabModel, PrefabRenderable,
    PrefabShadowPolicy,
};

pub(super) fn create_prefab_renderable_components(
    prefab: &ScenePrefabAsset,
    renderable: &openzt2_game_data::scene_prefab::PrefabRenderable,
) -> (
    PrefabRenderable,
    PrefabModel,
    PrefabMaterialOverrides,
    PrefabShadowPolicy,
) {
    let model = AssetId(renderable.model.0);
    let model_path = prefab
        .model_asset_path(model)
        .expect("validated prefab model dependency has a source path")
        .into();
    let overrides = renderable
        .material_overrides
        .iter()
        .map(|row| {
            let material = AssetId(row.material.0);
            PrefabMaterialOverride {
                material,
                material_path: prefab
                    .material_asset_path(material)
                    .expect("validated material override has a source path")
                    .into(),
                handle: None,
            }
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    (
        PrefabRenderable,
        PrefabModel {
            model,
            model_path,
            handle: None,
            scene_name: (!renderable.scene_name.is_empty())
                .then(|| renderable.scene_name.clone().into_boxed_str()),
        },
        PrefabMaterialOverrides(overrides),
        PrefabShadowPolicy {
            casts: renderable.visibility.0 & ScenePrefabRenderableVisibilityFlags::CAST_SHADOW.0
                != 0,
            receives: renderable.visibility.0
                & ScenePrefabRenderableVisibilityFlags::RECEIVE_SHADOW.0
                != 0,
            visible_in_reflections: renderable.visibility.0
                & ScenePrefabRenderableVisibilityFlags::REFLECTION_VISIBLE.0
                != 0,
        },
    )
}
