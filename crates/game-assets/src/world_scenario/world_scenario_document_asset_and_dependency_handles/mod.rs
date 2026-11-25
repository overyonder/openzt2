//! One canonical world-scenario document and its demand-loaded dependencies.

use bevy::{
    asset::{UntypedAssetId, VisitAssetDependencies},
    prelude::*,
};
use openzt2_game_data::{
    AssetId,
    world_scenario::{PhotoChallengeScriptBinding, ScenarioScriptBinding, WorldScenarioDocument},
};

use crate::{
    lua_script::lua_script_asset_loading::LuaScriptAsset, scene_prefab::ScenePrefabAsset,
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
};

#[derive(Debug)]
pub(super) struct DemandLoadedAssetHandle<T: Asset> {
    pub(super) id: AssetId,
    pub(super) handle: Handle<T>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DemandLoadedAssetPath {
    pub id: AssetId,
    pub path: Box<str>,
}

#[derive(TypePath, Debug)]
pub struct WorldScenarioDocumentAsset {
    pub authored: super::authored_world_scenario_state::AuthoredWorldScenarioState,
    pub document: WorldScenarioDocument,
    pub(super) starting_zoos: Box<[DemandLoadedAssetPath]>,
    pub(super) campaign_scenarios: Box<[DemandLoadedAssetHandle<WorldScenarioDocumentAsset>]>,
    pub(super) terrains: Box<[DemandLoadedAssetPath]>,
    pub(super) restored_terrain: Option<DemandLoadedAssetHandle<TerrainAsset>>,
    pub(super) scenes: Box<[DemandLoadedAssetHandle<ScenePrefabAsset>]>,
    pub(super) textures: Box<[DemandLoadedAssetHandle<Image>]>,
    pub(super) scripts: Box<[(ScenarioScriptBinding, String, Handle<LuaScriptAsset>)]>,
    pub(super) photo_scripts: Box<[(PhotoChallengeScriptBinding, String, Handle<LuaScriptAsset>)]>,
}

impl Asset for WorldScenarioDocumentAsset {}

impl VisitAssetDependencies for WorldScenarioDocumentAsset {
    fn visit_dependencies(&self, visit: &mut impl FnMut(UntypedAssetId)) {
        if let Some(terrain) = &self.restored_terrain {
            visit(terrain.handle.id().untyped());
        }
        for dependency in &self.scenes {
            visit(dependency.handle.id().untyped());
        }
        for dependency in &self.textures {
            visit(dependency.handle.id().untyped());
        }
        for (_, _, script) in &self.scripts {
            visit(script.id().untyped());
        }
        for (_, _, script) in &self.photo_scripts {
            visit(script.id().untyped());
        }
    }
}

impl WorldScenarioDocumentAsset {
    #[cfg(any(test, feature = "testing"))]
    pub fn from_test_document(document: WorldScenarioDocument) -> Self {
        Self {
            document,
            authored: Default::default(),
            starting_zoos: Box::new([]),
            campaign_scenarios: Box::new([]),
            terrains: Box::new([]),
            restored_terrain: None,
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

    #[cfg(any(test, feature = "testing"))]
    pub fn from_test_document_with_terrain_dependency(
        document: WorldScenarioDocument,
        terrain: AssetId,
        path: &str,
    ) -> Self {
        let mut asset = Self::from_test_document(document);
        asset.terrains = Box::new([DemandLoadedAssetPath {
            id: terrain,
            path: path.into(),
        }]);
        asset
    }

    pub fn former_fence_type(&self, starting_zoo: AssetId, persistent_id: u64) -> Option<AssetId> {
        self.authored
            .former_fence_types
            .get(&(starting_zoo, persistent_id))
            .copied()
    }

    pub fn terrain_path(&self, id: AssetId) -> Option<&str> {
        self.terrains
            .binary_search_by_key(&id, |candidate| candidate.id)
            .ok()
            .map(|index| self.terrains[index].path.as_ref())
    }

    pub fn restored_terrain(&self, id: AssetId) -> Option<&Handle<TerrainAsset>> {
        self.restored_terrain
            .as_ref()
            .filter(|terrain| terrain.id == id)
            .map(|terrain| &terrain.handle)
    }

    pub fn scene(&self, id: AssetId) -> Option<&Handle<ScenePrefabAsset>> {
        Self::handle(&self.scenes, id)
    }

    pub fn texture_image(&self, id: AssetId) -> Option<Handle<Image>> {
        Self::handle(&self.textures, id).cloned()
    }

    pub fn script(
        &self,
        binding: &ScenarioScriptBinding,
    ) -> Option<(&str, &Handle<LuaScriptAsset>)> {
        self.scripts
            .iter()
            .find(|(candidate, _, _)| candidate == binding)
            .map(|(_, path, handle)| (path.as_str(), handle))
    }

    pub fn validation_script(
        &self,
        scenario: AssetId,
    ) -> Option<(&ScenarioScriptBinding, &str, &Handle<LuaScriptAsset>)> {
        self.scripts.iter().find_map(|(binding, path, handle)| {
            (binding.scenario == scenario
                && binding.phase
                    == openzt2_game_data::world_scenario::ScenarioScriptPhase::Validate)
                .then_some((binding, path.as_str(), handle))
        })
    }

    pub fn photo_script(
        &self,
        binding: &PhotoChallengeScriptBinding,
    ) -> Option<(&str, &Handle<LuaScriptAsset>)> {
        self.photo_scripts
            .iter()
            .find(|(candidate, _, _)| candidate == binding)
            .map(|(_, path, handle)| (path.as_str(), handle))
    }
}
