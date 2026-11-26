//! Loading and dependency construction for one world-scenario source document.

use std::{io, path::Path};

use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};
use openzt2_game_data::AssetId;

use super::{
    world_scenario_document_asset_and_dependency_handles::{
        DemandLoadedAssetHandle, DemandLoadedAssetPath, ScenarioScriptDependencies,
        WorldScenarioDocumentAsset,
    },
    world_scenario_source_reference_resolution::{SourceReference, SourceReferenceKind},
};

use crate::{
    asset_source::{
        AssetArchives,
        archive_entry_asset_path::{
            ARCHIVE_ENTRY_ASSET_EXTENSION, archive_entry_asset_source_path,
        },
    },
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

pub(super) fn scenario_manager_asset_path(
    archives: &AssetArchives,
    path: &str,
) -> std::path::PathBuf {
    let path = std::path::PathBuf::from(AssetPath::new(path).key());
    archives
        .resolved_winning_entry(&path)
        .filter(|winner| winner.normalized_asset_path == path)
        .map_or(path, |winner| {
            crate::asset_source::archive_entry_asset_path::archive_entry_asset_path(&winner)
        })
}

pub fn load_scenario_manager_from_path(
    archives: &AssetArchives,
    server: &AssetServer,
    path: &str,
) -> Handle<WorldScenarioDocumentAsset> {
    server.load(scenario_manager_asset_path(archives, path))
}

impl AssetLoader for WorldScenarioDocumentLoader {
    type Asset = WorldScenarioDocumentAsset;
    type Settings = ();
    type Error = io::Error;

    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "load_world_scenario", skip_all)
    )]
    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let asset_path = context.path().path();
        let source_path = archive_entry_asset_source_path(asset_path).map(|(path, _, _)| path);
        let path = source_path
            .as_deref()
            .unwrap_or(asset_path)
            .to_string_lossy()
            .replace('\\', "/");
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
            for entry in source
                .root
                .element_children()
                .filter(|entry| entry.name.rsplit(':').next() == Some("entry"))
            {
                let Some(reference) = entry.attribute("scenario") else {
                    continue;
                };
                // A missing goal settles as an independent failed load. Keep
                // the campaign and its other goal bindings available.
                let resolved = self
                    .archives
                    .resolve_source_reference(Path::new(&path), reference)
                    .unwrap_or_else(|| {
                        let mut candidate =
                            z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(
                                Path::new(reference),
                            );
                        if candidate.extension().is_none() {
                            candidate.set_extension("xml");
                        }
                        if candidate.components().count() == 1 {
                            return Path::new(&path).with_file_name(&candidate);
                        }
                        candidate
                    });
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
        let (document, authored, references) = super::world_source_document_dependency_lowering::lower_world_source_document_and_resolve_dependencies(
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
        let manager = (source.root.name.rsplit(':').next() == Some("ZTScenarioMgr"))
            .then(|| document.scenarios.first().map(|record| record.id))
            .flatten();
        let mode_economy =
            super::original_saved_world_configuration_loading::load_world_mode_economy_policy(
                source,
                &self.archives,
                context,
            )
            .await?;
        Ok(WorldScenarioDocumentAsset {
            document,
            authored,
            manager,
            mode_economy,
            starting_zoos,
            campaign_scenarios: campaign_scenarios.into_boxed_slice(),
            terrains: terrains.into_boxed_slice(),
            restored_terrain: None,
            scenes: scenes.into_boxed_slice(),
            textures: textures.into_boxed_slice(),
            scripts: ScenarioScriptDependencies::new(scripts),
            photo_scripts,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["xml", "zt2", ARCHIVE_ENTRY_ASSET_EXTENSION]
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
