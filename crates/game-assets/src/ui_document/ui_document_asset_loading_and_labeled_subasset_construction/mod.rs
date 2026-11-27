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
    ui_document_role_asset_path_selection::UI_DOCUMENT_ASSET_EXTENSION,
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
        let asset_path = load_context.path().path();
        let primary_source_path = if asset_path
            .extension()
            .is_some_and(|extension| extension == UI_DOCUMENT_ASSET_EXTENSION)
        {
            asset_path.with_extension("")
        } else {
            asset_path.to_path_buf()
        }
        .to_string_lossy()
        .replace('\\', "/");
        let template_declaration_paths = self
            .template_declaration_path_index_cache
            .template_declaration_paths(&self.asset_archives);
        let lowered_ui_assets = super::source::lower(
            &primary_source_path,
            &primary_source_bytes,
            &self.asset_archives,
            &template_declaration_paths,
            load_context,
        )
        .await?;
        for (label, rail_camera_document) in lowered_ui_assets.rail_cameras {
            load_context.labeled_asset_scope(label, |label_context| {
                Ok::<_, io::Error>(
                    crate::scene_prefab::create_scene_prefab_asset_and_load_dependencies(
                        rail_camera_document,
                        label_context,
                    ),
                )
            })?;
        }
        let mut fame_stars = lowered_ui_assets.fame_stars;
        let mut first_person_hotkeys = lowered_ui_assets.first_person_hotkeys;
        let mut canonical_ui_documents = lowered_ui_assets.documents.into_iter();
        let root_ui_document = canonical_ui_documents.next().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "UI document has no lowered root",
            )
        })?;
        let labeled_documents = canonical_ui_documents
            .map(|document| {
                let label = match document.role {
                    UiDocumentRole::Fragment => {
                        format!("Fragment/{}", document.id.to_lowercase_hexadecimal_string())
                    }
                    role => format!("Role/{}", role.stable_key()),
                };
                (label, document)
            })
            .collect::<Vec<_>>();
        let fragment_labels = labeled_documents
            .iter()
            .filter(|(_, document)| document.role == UiDocumentRole::Fragment)
            .map(|(label, document)| (document.id, label.clone()))
            .collect::<BTreeMap<_, _>>();
        for (label, document) in labeled_documents {
            let stars = (document.role == UiDocumentRole::InGameHud)
                .then(|| fame_stars.take())
                .flatten();
            let hotkeys = if document.role == UiDocumentRole::InGameHud {
                std::mem::take(&mut first_person_hotkeys)
            } else {
                Vec::new()
            };
            load_context.labeled_asset_scope(label, |context| {
                Ok::<_, io::Error>(create_ui_document_asset_and_load_typed_dependencies(
                    document,
                    stars,
                    hotkeys,
                    &fragment_labels,
                    context,
                ))
            })?;
        }
        Ok(create_ui_document_asset_and_load_typed_dependencies(
            root_ui_document,
            fame_stars,
            first_person_hotkeys,
            &fragment_labels,
            load_context,
        ))
    }

    fn extensions(&self) -> &[&str] {
        &[UI_DOCUMENT_ASSET_EXTENSION]
    }
}
