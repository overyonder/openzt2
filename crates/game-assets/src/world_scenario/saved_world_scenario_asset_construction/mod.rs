//! Asset dependencies for a decoded world snapshot.

use std::io;

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::{
    world_scenario_document_asset_and_dependency_handles::{
        DemandLoadedAssetHandle, DemandLoadedAssetPath, WorldScenarioDocumentAsset,
    },
    world_scenario_document_source::WorldScenarioDocumentSource,
    world_scenario_document_source_encoding::validate_world_scenario_document_source,
};

pub fn add_restored_world_scenario_asset(
    server: &AssetServer,
    mut source: WorldScenarioDocumentSource,
    terrain_id: AssetId,
    terrain: Handle<TerrainAsset>,
) -> io::Result<Handle<WorldScenarioDocumentAsset>> {
    validate_world_scenario_document_source(&source)?;
    if !source
        .terrains
        .iter()
        .any(|dependency| dependency.id == terrain_id)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "saved world has no selected terrain dependency",
        ));
    }
    source
        .starting_zoos
        .sort_unstable_by_key(|dependency| dependency.id);
    source
        .terrains
        .sort_unstable_by_key(|dependency| dependency.id);
    let scenes = load_dependency_paths(server, source.scenes);
    let textures = load_dependency_paths(server, source.textures);
    let scripts = source
        .document
        .scenario_script_bindings
        .iter()
        .map(|binding| {
            let path = binding.script.clone();
            (binding.clone(), path.clone(), server.load(path))
        })
        .collect::<Box<[_]>>();
    let photo_scripts = source
        .document
        .photo_challenge_script_bindings
        .iter()
        .map(|binding| {
            let path = binding.script.clone();
            (binding.clone(), path.clone(), server.load(path))
        })
        .collect::<Box<[_]>>();
    Ok(server.add(WorldScenarioDocumentAsset {
        document: source.document,
        authored: source.authored,
        starting_zoos: source.starting_zoos,
        campaign_scenarios: Box::new([]),
        terrains: source.terrains,
        restored_terrain: Some(DemandLoadedAssetHandle {
            id: terrain_id,
            handle: terrain,
        }),
        scenes,
        textures,
        scripts,
        photo_scripts,
    }))
}

fn load_dependency_paths<T: Asset>(
    server: &AssetServer,
    mut paths: Box<[DemandLoadedAssetPath]>,
) -> Box<[DemandLoadedAssetHandle<T>]> {
    paths.sort_unstable_by_key(|dependency| dependency.id);
    paths
        .into_vec()
        .into_iter()
        .map(|dependency| DemandLoadedAssetHandle {
            id: dependency.id,
            handle: server.load(dependency.path.into_string()),
        })
        .collect()
}
