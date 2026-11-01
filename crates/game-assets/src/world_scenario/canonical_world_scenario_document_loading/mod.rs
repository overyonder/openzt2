//! Typed loading of persisted canonical world documents.

use super::{
    world_scenario_document_asset_and_dependency_handles::{
        DemandLoadedAssetHandle, DemandLoadedAssetPath, WorldScenarioDocumentAsset,
    },
    world_scenario_document_source::WorldScenarioDocumentSource,
    world_scenario_document_source_encoding::decode_world_scenario_document_source,
};
use crate::asset_source::AssetArchives;
use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};
use std::io;

#[derive(TypePath)]
pub(super) struct CanonicalWorldScenarioDocumentLoader {
    archives: AssetArchives,
}

impl FromWorld for CanonicalWorldScenarioDocumentLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            archives: world.resource::<AssetArchives>().clone(),
        }
    }
}

impl AssetLoader for CanonicalWorldScenarioDocumentLoader {
    type Asset = WorldScenarioDocumentAsset;
    type Settings = ();
    type Error = io::Error;
    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<Self::Asset> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let source = decode_world_scenario_document_source(&bytes)?;
        build_canonical_world_document_dependencies(source, &self.archives, context)
    }
    fn extensions(&self) -> &[&str] {
        &["ozworld"]
    }
}

fn build_canonical_world_document_dependencies(
    mut source: WorldScenarioDocumentSource,
    archives: &AssetArchives,
    context: &mut LoadContext<'_>,
) -> io::Result<WorldScenarioDocumentAsset> {
    for dependency in source
        .starting_zoos
        .iter_mut()
        .chain(source.terrains.iter_mut())
    {
        if dependency.path.starts_with("./") {
            let path = context
                .path()
                .resolve_embed_str(&dependency.path)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            dependency.path = path.to_string().into_boxed_str();
        }
    }
    for paths in [&mut source.starting_zoos, &mut source.terrains] {
        paths.sort_unstable_by_key(|path| path.id);
        if paths.windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "duplicate world dependency identity",
            ));
        }
    }
    let scenes = load_dependencies(&source.scenes, context)?;
    let textures = load_dependencies(&source.textures, context)?;
    let path = context.path().path().to_string_lossy().into_owned();
    let scripts =
        super::world_scenario_script_dependency_loading::load_scenario_script_dependencies(
            &source.document,
            archives,
            &path,
            context,
        )?;
    let photo_scripts =
        super::world_scenario_script_dependency_loading::load_photo_challenge_script_dependencies(
            &source.document,
            archives,
            &path,
            context,
        );
    Ok(WorldScenarioDocumentAsset {
        document: source.document,
        starting_zoos: source.starting_zoos,
        campaign_scenarios: Box::new([]),
        terrains: source.terrains,
        scenes,
        textures,
        scripts,
        photo_scripts,
    })
}

fn load_dependencies<T: Asset>(
    paths: &[DemandLoadedAssetPath],
    context: &mut LoadContext<'_>,
) -> io::Result<Box<[DemandLoadedAssetHandle<T>]>> {
    let mut dependencies = paths
        .iter()
        .map(|dependency| {
            Ok(DemandLoadedAssetHandle {
                id: dependency.id,
                handle: context.load(dependency.path.to_string()),
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    dependencies.sort_unstable_by_key(|dependency| dependency.id);
    if dependencies.windows(2).any(|pair| pair[0].id == pair[1].id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "duplicate world dependency identity",
        ));
    }
    Ok(dependencies.into_boxed_slice())
}
