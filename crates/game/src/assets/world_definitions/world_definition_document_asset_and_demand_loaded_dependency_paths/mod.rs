//! One lowered world-definition source document and its demand-loaded dependencies.

use bevy::prelude::*;
use openzt2_game_data::{world_definitions::document::WorldDefinitionDocument, AssetId};

pub(crate) const AUTHORED_GROUND_PATH_SURFACE_MASK_SOURCE_PATHS: [&str; 18] = [
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
    pub(super) textures: Box<[DemandLoadedDependencyPath]>,
    pub(super) models: Box<[DemandLoadedDependencyPath]>,
    pub(super) effects: Box<[DemandLoadedDependencyPath]>,
    pub(super) scenes: Box<[DemandLoadedDependencyPath]>,
    pub(super) animation_sets: Box<[DemandLoadedDependencyPath]>,
}

impl WorldDefinitionAsset {
    pub(crate) fn contains_ground_path_surface_definitions(&self) -> bool {
        self.document
            .paths
            .iter()
            .any(|definition| !definition.elevated)
    }

    pub(crate) fn automatic_placement_definition_needs_source_lowered_bounds(
        &self,
        definition: AssetId,
        bounds_xz: [[f32; 2]; 2],
    ) -> bool {
        self.document
            .placeables
            .iter()
            .find(|placeable| placeable.id == definition && placeable.automatic_footprint)
            .is_some_and(|placeable| {
                let mut resolved = placeable.clone();
                resolved.apply_source_lowered_automatic_placement_bounds(bounds_xz)
                    && *placeable != resolved
            })
    }

    pub(crate) fn apply_source_lowered_automatic_placement_bounds(
        &mut self,
        definition: AssetId,
        bounds_xz: [[f32; 2]; 2],
    ) -> bool {
        let Some(placeable) = self
            .document
            .placeables
            .iter_mut()
            .find(|placeable| placeable.id == definition && placeable.automatic_footprint)
        else {
            return false;
        };
        let mut resolved = placeable.clone();
        if !resolved.apply_source_lowered_automatic_placement_bounds(bounds_xz)
            || *placeable == resolved
        {
            return false;
        }
        *placeable = resolved;
        true
    }

    pub(crate) fn simulation_timing(
        &self,
    ) -> Option<&openzt2_game_data::world_definitions::simulation_time::SimulationTimingDefinition>
    {
        self.document.simulation_timing.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn from_test_document(document: WorldDefinitionDocument) -> Self {
        Self {
            document,
            textures: Box::new([]),
            models: Box::new([]),
            effects: Box::new([]),
            scenes: Box::new([]),
            animation_sets: Box::new([]),
        }
    }

    fn load<T: Asset>(
        server: &AssetServer,
        paths: &[DemandLoadedDependencyPath],
        id: AssetId,
    ) -> Option<Handle<T>> {
        paths
            .binary_search_by_key(&id, |candidate| candidate.id)
            .ok()
            .map(|index| server.load(paths[index].path.to_string()))
    }

    pub(super) fn texture(&self, server: &AssetServer, id: AssetId) -> Option<Handle<Image>> {
        Self::load(server, &self.textures, id)
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
