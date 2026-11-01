//! Loading and dependency construction for one world-scenario source document.

use std::{io, path::Path};

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};
use openzt2_game_data::AssetId;

use super::{
    world_scenario_document_asset_and_dependency_handles::{
        DemandLoadedAssetHandle, DemandLoadedAssetPath, WorldScenarioDocumentAsset,
    },
    world_scenario_source_reference_resolution::{SourceReference, SourceReferenceKind},
};

use crate::{
    asset_source::AssetArchives,
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    },
};

#[derive(TypePath)]
pub(super) struct WorldScenarioDocumentLoader {
    archives: AssetArchives,
}

impl FromWorld for WorldScenarioDocumentLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            archives: world.resource::<AssetArchives>().clone(),
        }
    }
}

impl AssetLoader for WorldScenarioDocumentLoader {
    type Asset = WorldScenarioDocumentAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let _performance_timer = self
            .archives
            .measure_scene_loading_asset_translation("world_scenario");
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let path = context.path().path().to_string_lossy().replace('\\', "/");
        let source_documents = vec![
            parse_blue_fang_source_document(AssetPath::new(&path), &bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
        ];
        let source = &source_documents[0];
        let starting_zoos = if source.root.name.rsplit(':').next() == Some("maps") {
            let folder = if path.to_ascii_lowercase().contains("maps/scenario/") {
                ""
            } else {
                "default/"
            };
            source
                .root
                .element_children()
                .filter_map(|wrapper| {
                    let name = wrapper.name.rsplit(':').next()?;
                    let reference = format!("{folder}{name}.zt2");
                    let resolved = self
                        .archives
                        .resolve_source_reference(Path::new(&path), &reference)?;
                    Some(DemandLoadedAssetPath {
                        id: AssetId::from_key(&format!("start:{name}")),
                        path: resolved
                            .to_string_lossy()
                            .replace('\\', "/")
                            .into_boxed_str(),
                    })
                })
                .collect::<Box<[_]>>()
        } else {
            Box::new([])
        };
        let mut campaign_scenarios = Vec::new();
        if source.root.name.rsplit(':').next() == Some("BFCampaign") {
            for entry in source.root.element_children() {
                let reference = entry.attribute("scenario").ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{path} campaign entry has no scenario path"),
                    )
                })?;
                let resolved = self
                    .archives
                    .resolve_source_reference(Path::new(&path), reference)
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::NotFound,
                            format!("{path} references missing scenario document {reference}"),
                        )
                    })?;
                let id = AssetId::from_key(entry.attribute("key").ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("{path} campaign entry has no key"),
                    )
                })?);
                campaign_scenarios.push(DemandLoadedAssetHandle {
                    id,
                    handle: context.load(resolved),
                });
            }
        }
        let (document,references) = super::world_source_document_dependency_lowering::lower_world_source_document_and_resolve_dependencies(
            source,&self.archives,context,
        ).await?;
        let terrains = demand_loaded_asset_paths(&references, SourceReferenceKind::Terrain, |id| {
            document.maps.iter().any(|record| record.terrain == id)
        });
        let scenes = demand_handles(&references, SourceReferenceKind::Scene, context, |id| {
            document
                .starting_zoos
                .iter()
                .any(|start| start.entities.iter().any(|entity| entity.prefab == id))
        });
        let textures = demand_handles(&references, SourceReferenceKind::Texture, context, |id| {
            document.maps.iter().any(|record| record.thumbnail == id)
        });
        let scripts =
            super::world_scenario_script_dependency_loading::load_scenario_script_dependencies(
                &document,
                &self.archives,
                &path,
                context,
            )?;
        let photo_scripts = super::world_scenario_script_dependency_loading::load_photo_challenge_script_dependencies(
            &document,&self.archives,&path,context,
        );
        Ok(WorldScenarioDocumentAsset {
            document,
            starting_zoos,
            campaign_scenarios: campaign_scenarios.into_boxed_slice(),
            terrains: terrains.into_boxed_slice(),
            scenes: scenes.into_boxed_slice(),
            textures: textures.into_boxed_slice(),
            scripts,
            photo_scripts,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["xml", "zt2"]
    }
}

fn demand_handles<T: Asset>(
    references: &[SourceReference],
    kind: SourceReferenceKind,
    context: &mut LoadContext<'_>,
    used: impl Fn(AssetId) -> bool,
) -> Vec<DemandLoadedAssetHandle<T>> {
    let mut handles = references
        .iter()
        .filter(|reference| reference.kind == kind)
        .filter(|reference| used(reference.id))
        .map(|reference| DemandLoadedAssetHandle {
            id: reference.id,
            handle: context.load(reference.path.clone()),
        })
        .collect::<Vec<_>>();
    handles.sort_unstable_by_key(|handle| handle.id);
    handles.dedup_by_key(|handle| handle.id);
    handles
}

fn demand_loaded_asset_paths(
    references: &[SourceReference],
    kind: SourceReferenceKind,
    used: impl Fn(AssetId) -> bool,
) -> Vec<DemandLoadedAssetPath> {
    let mut paths = references
        .iter()
        .filter(|reference| reference.kind == kind)
        .filter(|reference| used(reference.id))
        .map(|reference| DemandLoadedAssetPath {
            id: reference.id,
            path: reference.path.clone().into_boxed_str(),
        })
        .collect::<Vec<_>>();
    paths.sort_unstable_by_key(|path| path.id);
    paths.dedup_by_key(|path| path.id);
    paths
}
