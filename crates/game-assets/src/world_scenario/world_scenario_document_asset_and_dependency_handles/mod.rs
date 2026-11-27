//! One canonical world-scenario document and its demand-loaded dependencies.

use std::{
    collections::BTreeMap,
    hash::{DefaultHasher, Hash, Hasher},
};

use bevy::{
    asset::{UntypedAssetId, VisitAssetDependencies},
    prelude::*,
};
use openzt2_game_data::{
    world_scenario::{
        PhotoChallengeScriptBinding, ScenarioScriptBinding, ScenarioScriptPhase,
        WorldScenarioDocument,
    },
    AssetId,
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

type ScenarioScript = (ScenarioScriptBinding, String, Handle<LuaScriptAsset>);

#[derive(Debug, Default)]
pub(super) struct ScenarioScriptDependencies {
    rows: Box<[ScenarioScript]>,
    by_binding: Box<[(u64, usize)]>,
    validation: BTreeMap<AssetId, usize>,
}

impl ScenarioScriptDependencies {
    pub(super) fn new(rows: Box<[ScenarioScript]>) -> Self {
        let mut by_binding = Vec::with_capacity(rows.len());
        let mut validation = BTreeMap::new();
        for (index, (binding, _, _)) in rows.iter().enumerate() {
            by_binding.push((Self::key(binding), index));
            if binding.phase == ScenarioScriptPhase::Validate {
                validation.entry(binding.scenario).or_insert(index);
            }
        }
        by_binding.sort_unstable();
        Self {
            rows,
            by_binding: by_binding.into_boxed_slice(),
            validation,
        }
    }

    fn key(binding: &ScenarioScriptBinding) -> u64 {
        let mut hasher = DefaultHasher::new();
        (
            binding.scenario,
            binding.objective,
            binding.phase as u8,
            &binding.script,
            &binding.entry,
        )
            .hash(&mut hasher);
        hasher.finish()
    }

    fn find(&self, binding: &ScenarioScriptBinding) -> Option<&ScenarioScript> {
        let key = Self::key(binding);
        let start = self
            .by_binding
            .partition_point(|&(candidate, _)| candidate < key);
        self.by_binding[start..]
            .iter()
            .take_while(|&&(candidate, _)| candidate == key)
            .map(|&(_, index)| &self.rows[index])
            .find(|(candidate, _, _)| candidate == binding)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WorldModeEconomyPolicy {
    pub initial_cash_dollars: Option<f32>,
    pub unlimited_cash: Option<bool>,
}

#[derive(TypePath, Debug)]
pub struct WorldScenarioDocumentAsset {
    pub authored: super::authored_world_scenario_state::AuthoredWorldScenarioState,
    pub document: WorldScenarioDocument,
    pub(super) manager: Option<AssetId>,
    pub(super) mode_economy: Option<WorldModeEconomyPolicy>,
    pub(super) starting_zoos: Box<[DemandLoadedAssetPath]>,
    pub(super) campaign_scenarios: Box<[DemandLoadedAssetHandle<WorldScenarioDocumentAsset>]>,
    pub(super) terrains: Box<[DemandLoadedAssetPath]>,
    pub(super) restored_terrain: Option<DemandLoadedAssetHandle<TerrainAsset>>,
    pub(super) scenes: Box<[DemandLoadedAssetHandle<ScenePrefabAsset>]>,
    pub(super) textures: Box<[DemandLoadedAssetHandle<Image>]>,
    pub(super) scripts: ScenarioScriptDependencies,
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
        for (_, _, script) in &self.scripts.rows {
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
            manager: None,
            mode_economy: None,
            authored: Default::default(),
            starting_zoos: Box::new([]),
            campaign_scenarios: Box::new([]),
            terrains: Box::new([]),
            restored_terrain: None,
            scenes: Box::new([]),
            textures: Box::new([]),
            scripts: ScenarioScriptDependencies::default(),
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

    pub fn manager(&self) -> Option<&openzt2_game_data::world_scenario::ScenarioDefinitionRecord> {
        self.document.find_scenario_record(self.manager?)
    }

    pub fn mode_economy(&self) -> Option<WorldModeEconomyPolicy> {
        self.mode_economy
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
            .find(binding)
            .map(|(_, path, handle)| (path.as_str(), handle))
    }

    pub fn validation_script(
        &self,
        scenario: AssetId,
    ) -> Option<(&ScenarioScriptBinding, &str, &Handle<LuaScriptAsset>)> {
        let (binding, path, handle) = self
            .scripts
            .rows
            .get(*self.scripts.validation.get(&scenario)?)?;
        Some((binding, path, handle))
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
