//! One canonical world-scenario document and its demand-loaded dependencies.

use bevy::prelude::*;
use openzt2_game_data::{
    world_scenario::{PhotoChallengeScriptBinding, ScenarioScriptBinding, WorldScenarioDocument},
    AssetId,
};

use crate::assets::{
    lua_script::lua_script_asset_loading::LuaScriptAsset, scene_prefab::ScenePrefabAsset,
};

#[derive(Debug)]
pub(super) struct DemandLoadedAssetHandle<T: Asset> {
    pub(super) id: AssetId,
    pub(super) handle: Handle<T>,
}

#[derive(Debug)]
pub(super) struct DemandLoadedAssetPath {
    pub(super) id: AssetId,
    pub(super) path: Box<str>,
}

#[derive(Asset, TypePath, Debug)]
pub struct WorldScenarioDocumentAsset {
    pub document: WorldScenarioDocument,
    pub(super) starting_zoos: Box<[DemandLoadedAssetPath]>,
    pub(super) campaign_scenarios: Box<[DemandLoadedAssetHandle<WorldScenarioDocumentAsset>]>,
    pub(super) terrains: Box<[DemandLoadedAssetPath]>,
    pub(super) scenes: Box<[DemandLoadedAssetHandle<ScenePrefabAsset>]>,
    pub(super) textures: Box<[DemandLoadedAssetHandle<Image>]>,
    pub(super) scripts: Box<[(ScenarioScriptBinding, String, Handle<LuaScriptAsset>)]>,
    pub(super) photo_scripts: Box<[(PhotoChallengeScriptBinding, String, Handle<LuaScriptAsset>)]>,
}

impl WorldScenarioDocumentAsset {
    #[cfg(test)]
    pub(crate) fn from_test_document(document: WorldScenarioDocument) -> Self {
        Self {
            document,
            starting_zoos: Box::new([]),
            campaign_scenarios: Box::new([]),
            terrains: Box::new([]),
            scenes: Box::new([]),
            textures: Box::new([]),
            scripts: Box::new([]),
            photo_scripts: Box::new([]),
        }
    }

    fn handle<T: Asset>(handles: &[DemandLoadedAssetHandle<T>], id: AssetId) -> Option<&Handle<T>> {
        handles
            .binary_search_by_key(&id, |candidate| candidate.id)
            .ok()
            .map(|index| &handles[index].handle)
    }

    pub(crate) fn terrain_path(&self, id: AssetId) -> Option<&str> {
        self.terrains
            .binary_search_by_key(&id, |candidate| candidate.id)
            .ok()
            .map(|index| self.terrains[index].path.as_ref())
    }

    pub fn scene(&self, id: AssetId) -> Option<&Handle<ScenePrefabAsset>> {
        Self::handle(&self.scenes, id)
    }

    pub fn texture_image(&self, id: AssetId) -> Option<Handle<Image>> {
        Self::handle(&self.textures, id).cloned()
    }

    pub(crate) fn script(
        &self,
        binding: &ScenarioScriptBinding,
    ) -> Option<(&str, &Handle<LuaScriptAsset>)> {
        self.scripts
            .iter()
            .find(|(candidate, _, _)| candidate == binding)
            .map(|(_, path, handle)| (path.as_str(), handle))
    }

    pub(crate) fn photo_script(
        &self,
        binding: &PhotoChallengeScriptBinding,
    ) -> Option<(&str, &Handle<LuaScriptAsset>)> {
        self.photo_scripts
            .iter()
            .find(|(candidate, _, _)| candidate == binding)
            .map(|(_, path, handle)| (path.as_str(), handle))
    }
}
