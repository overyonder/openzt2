use super::{
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    world_definition_source_document_lowering::lower_resolved_world_definition_source_document_closure_to_canonical_document,
};
use crate::asset_source::AssetArchives;
use crate::assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};
use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};
use std::{io, path::Path};

mod presentation_dependencies;
mod source_documents;
mod source_reference_discovery;

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

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let _performance_timer = self
            .archives
            .measure_scene_loading_asset_translation("world_definition");
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let path = context.path().path().to_string_lossy().replace('\\', "/");
        let authored_type_registry_source_order = self
            .archives
            .first_enabled_archive_entry_order(Path::new(&path))
            .unwrap_or([u64::MAX; 2]);
        let primary = parse_blue_fang_source_document(AssetPath::new(&path), &bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let documents = source_documents::load_world_definition_source_documents(
            &self.archives,
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
        let document =
            lower_resolved_world_definition_source_document_closure_to_canonical_document(
                &documents,
                &actor_manifest_resolution_index,
                &path,
                authored_type_registry_source_order,
            )?
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unsupported world-definition source {path}"),
                )
            })?;
        presentation_dependencies::register_world_definition_presentation_dependencies(
            &self.archives,
            &path,
            &documents,
            document,
            &actor_manifest_resolution_index,
            &resolved_models,
        )
    }

    fn extensions(&self) -> &[&str] {
        &["xml", "dl", "maxml", "zt2", "trk", "old"]
    }
}

fn unresolved_reference(from: &str, kind: &str, reference: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{from} references missing {kind} {reference}"),
    )
}
