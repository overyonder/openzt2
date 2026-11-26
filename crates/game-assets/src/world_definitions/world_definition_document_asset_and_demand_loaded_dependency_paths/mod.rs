//! One lowered world-definition source document and its demand-loaded dependencies.

use bevy::prelude::*;
use openzt2_game_data::{world_definitions::document::WorldDefinitionDocument, AssetId};

pub const AUTHORED_GROUND_PATH_SURFACE_MASK_SOURCE_PATHS: [&str; 18] = [
    "entities/objects/paths/shared/north_mask.dds",
    "entities/objects/paths/shared/north_mask1.dds",
    "entities/objects/paths/shared/north_mask2.dds",
    "entities/objects/paths/shared/north_mask8.dds",
    "entities/objects/paths/shared/north_mask9.dds",
    "entities/objects/paths/shared/north_mask13.dds",
    "entities/objects/paths/shared/north_mask15.dds",
    "entities/objects/paths/shared/north_mask23.dds",
    "entities/objects/paths/shared/north_mask26.dds",
    "entities/objects/paths/shared/west_mask.dds",
    "entities/objects/paths/shared/west_mask2.dds",
    "entities/objects/paths/shared/west_mask3.dds",
    "entities/objects/paths/shared/west_mask6.dds",
    "entities/objects/paths/shared/west_mask7.dds",
    "entities/objects/paths/shared/west_mask17.dds",
    "entities/objects/paths/shared/west_mask19.dds",
    "entities/objects/paths/shared/west_mask25.dds",
    "entities/objects/paths/shared/west_mask28.dds",
];

#[derive(Debug)]
pub(super) struct DemandLoadedDependencyPath {
    pub(super) id: AssetId,
    pub(super) path: Box<str>,
}

#[derive(Asset, TypePath, Debug)]
pub struct WorldDefinitionAsset {
    pub(super) document: WorldDefinitionDocument,
    pub(super) timekeeper: Option<super::timing::AuthoredTimekeeperPolicy>,
    // Directory membership and unselected binder declarations also determine
    // reference uniqueness; selected-file dependencies alone cannot track them.
    pub(super) authored_binder_sources:
        Option<std::sync::Arc<super::world_definition_asset_loading::authored_binder_reference_resolution::AuthoredBinderSources>>,
    pub(super) textures: Box<[DemandLoadedDependencyPath]>,
    pub(super) models: Box<[DemandLoadedDependencyPath]>,
    pub(super) materials: Box<[DemandLoadedDependencyPath]>,
    pub(super) effects: Box<[DemandLoadedDependencyPath]>,
    pub(super) scenes: Box<[DemandLoadedDependencyPath]>,
    pub(super) animation_sets: Box<[DemandLoadedDependencyPath]>,
}

impl WorldDefinitionAsset {
    pub fn simulation_timing(
        &self,
    ) -> Option<&openzt2_game_data::world_definitions::simulation_time::SimulationTimingDefinition>
    {
        self.document.simulation_timing.as_ref()
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn from_test_document(document: WorldDefinitionDocument) -> Self {
        Self {
            document,
            timekeeper: None,
            authored_binder_sources: None,
            textures: Box::new([]),
            models: Box::new([]),
            materials: Box::new([]),
            effects: Box::new([]),
            scenes: Box::new([]),
            animation_sets: Box::new([]),
        }
    }

    /// Lets a fixture demand-load scene `id` from `path`.
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn with_test_scene(mut self, id: AssetId, path: &str) -> Self {
        let mut scenes = self.scenes.into_vec();
        scenes.push(DemandLoadedDependencyPath {
            id,
            path: path.into(),
        });
        scenes.sort_by_key(|scene| scene.id);
        self.scenes = scenes.into_boxed_slice();
        self
    }

    fn dependency_path(paths: &[DemandLoadedDependencyPath], id: AssetId) -> Option<&str> {
        paths
            .binary_search_by_key(&id, |candidate| candidate.id)
            .ok()
            .map(|index| &*paths[index].path)
    }

    fn load<T: Asset>(
        server: &AssetServer,
        paths: &[DemandLoadedDependencyPath],
        id: AssetId,
    ) -> Option<Handle<T>> {
        Some(server.load(Self::dependency_path(paths, id)?.to_owned()))
    }

    pub(super) fn texture(&self, server: &AssetServer, id: AssetId) -> Option<Handle<Image>> {
        Self::load(server, &self.textures, id)
    }

    pub(super) fn texture_path(&self, id: AssetId) -> Option<&str> {
        Self::dependency_path(&self.textures, id)
    }

    pub(super) fn material(
        &self,
        server: &AssetServer,
        id: AssetId,
    ) -> Option<Handle<crate::material::material_asset_types::MaterialAsset>> {
        Self::load(server, &self.materials, id)
    }

    pub(super) fn model<T: Asset>(&self, server: &AssetServer, id: AssetId) -> Option<Handle<T>> {
        Self::load(server, &self.models, id)
    }

    pub(super) fn effect<T: Asset>(&self, server: &AssetServer, id: AssetId) -> Option<Handle<T>> {
        Self::load(server, &self.effects, id)
    }

    pub(super) fn scene<T: Asset>(&self, server: &AssetServer, id: AssetId) -> Option<Handle<T>> {
        Self::load(server, &self.scenes, id)
    }

    pub(super) fn animation_set<T: Asset>(
        &self,
        server: &AssetServer,
        id: AssetId,
    ) -> Option<Handle<T>> {
        Self::load(server, &self.animation_sets, id)
    }
}
