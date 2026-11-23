//! Canonical world documents and the source paths of their asset dependencies.

use super::world_scenario_document_asset_and_dependency_handles::{
    DemandLoadedAssetPath, WorldScenarioDocumentAsset,
};
use bevy::prelude::*;
use openzt2_game_data::{AssetId, world_scenario::WorldScenarioDocument};
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldScenarioDocumentSource {
    pub document: WorldScenarioDocument,
    pub starting_zoos: Box<[DemandLoadedAssetPath]>,
    pub terrains: Box<[DemandLoadedAssetPath]>,
    pub scenes: Box<[DemandLoadedAssetPath]>,
    pub textures: Box<[DemandLoadedAssetPath]>,
    pub authored: super::authored_world_scenario_state::AuthoredWorldScenarioState,
}

impl WorldScenarioDocumentAsset {
    pub fn capture_source_document(
        &self,
        server: &AssetServer,
    ) -> io::Result<WorldScenarioDocumentSource> {
        let dependency_path = |id: AssetId, handle: bevy::asset::UntypedAssetId| {
            let path = server.get_path(handle).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "world dependency has no source path",
                )
            })?;
            Ok(DemandLoadedAssetPath {
                id,
                path: path.to_string().into_boxed_str(),
            })
        };
        let mut document = self.document.clone();
        for binding in &mut document.scenario_script_bindings {
            let (path, _) = self.script(binding).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "scenario script has no source path",
                )
            })?;
            binding.script = path.to_owned();
        }
        document
            .photo_challenge_script_bindings
            .retain_mut(|binding| {
                let Some((path, _)) = self.photo_script(binding) else {
                    return false;
                };
                binding.script = path.to_owned();
                true
            });
        Ok(WorldScenarioDocumentSource {
            document,
            authored: self.authored.clone(),
            starting_zoos: self.starting_zoos.clone(),
            terrains: self.terrains.clone(),
            scenes: self
                .scenes
                .iter()
                .map(|dependency| dependency_path(dependency.id, dependency.handle.id().untyped()))
                .collect::<io::Result<Box<[_]>>>()?,
            textures: self
                .textures
                .iter()
                .map(|dependency| dependency_path(dependency.id, dependency.handle.id().untyped()))
                .collect::<io::Result<Box<[_]>>>()?,
        })
    }
}
