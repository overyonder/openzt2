//! UI source lowering and ordered labeled-subasset construction.

use std::{collections::BTreeMap, io};

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};
use openzt2_game_data::ui_document::document::UiDocumentRole;

use super::{
    ui_document_asset_dependency_loading::create_ui_document_asset_and_load_typed_dependencies,
    ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    ui_template_declaration_path_index_cache::UiTemplateDeclarationPathIndexCache,
};

#[derive(TypePath)]
pub(super) struct UiDocumentAssetLoader {
    asset_archives: crate::asset_source::AssetArchives,
    template_declaration_path_index_cache: UiTemplateDeclarationPathIndexCache,
}

impl FromWorld for UiDocumentAssetLoader {
    fn from_world(world: &mut World) -> Self {
        Self {
            asset_archives: world
                .resource::<crate::asset_source::AssetArchives>()
                .clone(),
            template_declaration_path_index_cache: Default::default(),
        }
    }
}

impl AssetLoader for UiDocumentAssetLoader {
    type Asset = UiDocumentAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut primary_source_bytes = Vec::new();
        reader.read_to_end(&mut primary_source_bytes).await?;
        let primary_source_path = load_context
            .path()
            .path()
            .to_string_lossy()
            .replace('\\', "/");
        let template_declaration_paths = self
            .template_declaration_path_index_cache
            .template_declaration_paths_for_current_archive_revision(&self.asset_archives);
        let lowered_ui_assets = super::source::lower(
            &primary_source_path,
            &primary_source_bytes,
            &self.asset_archives,
            &template_declaration_paths,
            load_context,
        )
        .await?;
        for (label, rail_camera_document) in lowered_ui_assets.rail_cameras {
            let rail_camera_asset =
                crate::assets::scene_prefab::create_scene_prefab_asset_and_load_dependencies(
                    rail_camera_document,
                    load_context,
                );
            load_context.add_labeled_asset(label, rail_camera_asset);
        }
        let mut canonical_ui_documents = lowered_ui_assets.documents.into_iter();
        let root_ui_document = canonical_ui_documents.next().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "UI document has no lowered root",
            )
        })?;
        let remaining_ui_documents = canonical_ui_documents.collect::<Vec<_>>();
        let mut same_source_fragment_handles = BTreeMap::new();
        for canonical_ui_document in remaining_ui_documents
            .iter()
            .filter(|document| document.role == UiDocumentRole::Fragment)
            .cloned()
        {
            let asset_id = canonical_ui_document.id;
            let label = format!("Fragment/{}", asset_id.to_lowercase_hexadecimal_string());
            let fragment_asset = create_ui_document_asset_and_load_typed_dependencies(
                canonical_ui_document,
                &same_source_fragment_handles,
                load_context,
            );
            let fragment_handle = load_context.add_labeled_asset(label, fragment_asset);
            same_source_fragment_handles.insert(asset_id, fragment_handle);
        }
        for canonical_ui_document in remaining_ui_documents {
            if canonical_ui_document.role != UiDocumentRole::Fragment {
                let label = format!("Role/{}", canonical_ui_document.role.stable_key());
                let role_asset = create_ui_document_asset_and_load_typed_dependencies(
                    canonical_ui_document,
                    &same_source_fragment_handles,
                    load_context,
                );
                load_context.add_labeled_asset(label, role_asset);
            }
        }
        Ok(create_ui_document_asset_and_load_typed_dependencies(
            root_ui_document,
            &same_source_fragment_handles,
            load_context,
        ))
    }

    fn extensions(&self) -> &[&str] {
        &["xml", "zt2", "old"]
    }
}
