use super::{
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    world_definition_source_document_lowering::lower_resolved_world_definition_source_document_closure_to_canonical_document,
};
use crate::asset_source::{
    AssetArchives,
    archive_entry_asset_path::{ARCHIVE_ENTRY_ASSET_EXTENSION, archive_entry_asset_source_path},
};
use crate::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};
use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};
use std::{io, path::Path, sync::Arc};

pub mod authored_binder_reference_resolution;
mod fossil_puzzle_slots;
pub mod presentation_dependencies;
pub mod source_documents;
pub mod source_reference_discovery;

#[derive(TypePath)]
pub(super) struct WorldDefinitionAssetLoader {
    archives: AssetArchives,
}

impl FromWorld for WorldDefinitionAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            archives: world.resource::<AssetArchives>().clone(),
        }
    }
}

impl AssetLoader for WorldDefinitionAssetLoader {
    type Asset = WorldDefinitionAsset;
    type Settings = ();
    type Error = io::Error;

    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "load_world_definition", skip_all)
    )]
    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let binder_sources = self.archives.authored_binder_sources();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let physical_path = archive_entry_asset_source_path(context.path().path())
            .map_or_else(|| context.path().path().to_owned(), |(path, _, _)| path);
        let path = physical_path.to_string_lossy().replace('\\', "/");
        let authored_type_registry_source_order = self
            .archives
            .first_enabled_archive_entry_order(Path::new(&path))
            .unwrap_or([u64::MAX; 2]);
        let primary = parse_blue_fang_source_document(AssetPath::new(&path), &bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let (documents, uses_binder_sources) =
            source_documents::load_world_definition_source_documents(
                &self.archives,
                &binder_sources,
                context,
                &path,
                primary,
            )
            .await?;
        let (actor_manifest_resolution_index, resolved_models) =
            presentation_dependencies::resolve_world_definition_presentation_models(
                &self.archives,
                context,
                &path,
                &documents,
            )
            .await?;
        let mut timekeeper = None;
        let mut document =
            lower_resolved_world_definition_source_document_closure_to_canonical_document(
                &documents,
                &actor_manifest_resolution_index,
                &path,
                authored_type_registry_source_order,
                &mut timekeeper,
            )?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unsupported world-definition source {path}"),
                )
            })?;
        let puzzle_textures = fossil_puzzle_slots::lower_fossil_puzzle_background_slots(
            &self.archives,
            &binder_sources,
            context,
            &path,
            &documents,
            &mut document,
        )
        .await?;
        let mut asset =
            presentation_dependencies::register_world_definition_presentation_dependencies(
                &self.archives,
                &path,
                &documents,
                document,
                &actor_manifest_resolution_index,
                &resolved_models,
            )?;
        asset.timekeeper = timekeeper;
        let uses_puzzle_background = puzzle_textures.is_some();
        if let Some(puzzle_textures) = puzzle_textures {
            let mut textures = asset.textures.into_vec();
            textures.extend(puzzle_textures);
            textures.sort_unstable_by_key(|texture| texture.id);
            textures.dedup_by_key(|texture| texture.id);
            asset.textures = textures.into_boxed_slice();
        }
        if uses_binder_sources || uses_puzzle_background {
            if !Arc::ptr_eq(&binder_sources, &self.archives.authored_binder_sources()) {
                return Err(io::Error::other(
                    "authored binder sources changed while loading",
                ));
            }
            asset.authored_binder_sources = Some(binder_sources);
        }
        Ok(asset)
    }

    fn extensions(&self) -> &[&str] {
        &[
            "xml",
            "dl",
            "maxml",
            "zt2",
            "trk",
            "old",
            ARCHIVE_ENTRY_ASSET_EXTENSION,
        ]
    }
}

fn unresolved_reference(from: &str, kind: &str, reference: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{from} references missing {kind} {reference}"),
    )
}
