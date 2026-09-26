//! Prefab render components.

use bevy::{gltf::Gltf, prelude::*};
use openzt2_game_data::AssetId;

use crate::assets::{
    material::material_asset_types::MaterialAsset, scene_prefab::ScenePrefabAsset,
};

/// Renders a prefab beneath an existing entity. The handle keeps its assets
/// alive while the render hierarchy loads.
#[derive(Component, Debug, Clone)]
pub(crate) struct PrefabPresentation(pub(crate) Handle<ScenePrefabAsset>);

impl PrefabPresentation {
    pub(crate) const fn new(prefab: Handle<ScenePrefabAsset>) -> Self {
        Self(prefab)
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct PrefabPresentationHydrated;

/// Direct model handle for one authored renderable.
#[derive(Component, Debug, Clone)]
pub(crate) struct PrefabModel {
    pub(crate) model: AssetId,
    pub(crate) model_path: Box<str>,
    pub(crate) handle: Option<Handle<Gltf>>,
    pub(crate) scene_name: Option<Box<str>>,
}

/// One authored render primitive represented by an ordinary ECS entity.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PrefabRenderable;

/// Authoritative material substitutions for one renderable.
#[derive(Component, Debug, Clone)]
pub(crate) struct PrefabMaterialOverrides(pub(crate) Box<[PrefabMaterialOverride]>);

#[derive(Debug, Clone)]
pub(crate) struct PrefabMaterialOverride {
    pub(crate) material: AssetId,
    pub(crate) material_path: Box<str>,
    pub(crate) handle: Option<Handle<MaterialAsset>>,
}

/// Authored render visibility not represented by hierarchical `Visibility`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrefabShadowPolicy {
    pub(crate) casts: bool,
    pub(crate) receives: bool,
    pub(crate) visible_in_reflections: bool,
}
