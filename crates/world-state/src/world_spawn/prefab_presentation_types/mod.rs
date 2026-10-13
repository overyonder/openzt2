//! Prefab render components.

use bevy::gltf::Gltf;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;

/// Renders a prefab beneath an existing entity. The handle keeps its assets
/// alive while the render hierarchy loads.
#[derive(Component, Debug, Clone)]
pub struct PrefabPresentation(pub Handle<ScenePrefabAsset>);

impl PrefabPresentation {
    pub const fn new(prefab: Handle<ScenePrefabAsset>) -> Self {
        Self(prefab)
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PrefabPresentationHydrated;

/// Direct model handle for one authored renderable.
#[derive(Component, Debug, Clone)]
pub struct PrefabModel {
    pub model: AssetId,
    pub model_path: Box<str>,
    pub handle: Option<Handle<Gltf>>,
    pub scene_name: Option<Box<str>>,
    /// Lowercase authored source material name used by texture replacements.
    pub material_name: Option<Box<str>>,
}

/// One authored render primitive represented by an ordinary ECS entity.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PrefabRenderable;

/// Authoritative material substitutions for one renderable.
#[derive(Component, Debug, Clone)]
pub struct PrefabMaterialOverrides(pub Box<[PrefabMaterialOverride]>);

#[derive(Debug, Clone)]
pub struct PrefabMaterialOverride {
    pub material: AssetId,
    pub material_path: Box<str>,
    pub handle: Option<Handle<MaterialAsset>>,
}

/// Authored render visibility not represented by hierarchical `Visibility`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrefabShadowPolicy {
    pub casts: bool,
    pub receives: bool,
    pub visible_in_reflections: bool,
}

/// Instance-owned clothing substitutions, retained when model scenes are rebuilt.
#[derive(Component, Debug, Clone, Default)]
pub struct PrefabClothingTextures(pub Vec<(AssetId, Handle<Image>)>);
