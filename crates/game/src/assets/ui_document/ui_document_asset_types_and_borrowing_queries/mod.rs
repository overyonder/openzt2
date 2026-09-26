//! One UI document and its sorted typed dependency-handle indexes.

use bevy::{audio::AudioSource, prelude::*};
use openzt2_game_data::{ui_document::document::UiDocument, AssetId};

use crate::assets::{
    scene_prefab::ScenePrefabAsset,
    texture::interactive_texture_metadata_asset_and_borrowing_queries::InteractiveTextureMetadataAsset,
};

#[derive(Asset, TypePath, Clone, Debug)]
pub(crate) struct UiDocumentAsset {
    pub(in crate::assets::ui_document) canonical_ui_document: UiDocument,
    pub(in crate::assets::ui_document) texture_image_handles: Box<[(AssetId, Handle<Image>)]>,
    pub(in crate::assets::ui_document) interactive_texture_metadata_handles:
        Box<[(AssetId, Handle<InteractiveTextureMetadataAsset>)]>,
    pub(in crate::assets::ui_document) scene_prefab_handles:
        Box<[(AssetId, Handle<ScenePrefabAsset>)]>,
    pub(in crate::assets::ui_document) nested_ui_document_handles:
        Box<[(AssetId, Handle<UiDocumentAsset>)]>,
    pub(in crate::assets::ui_document) audio_source_handles: Box<[(AssetId, Handle<AudioSource>)]>,
}

impl UiDocumentAsset {
    pub(crate) fn canonical_ui_document(&self) -> &UiDocument {
        &self.canonical_ui_document
    }

    pub(crate) fn texture_image_handle(&self, asset_id: AssetId) -> Option<&Handle<Image>> {
        find_typed_dependency_handle(&self.texture_image_handles, asset_id)
    }

    pub(crate) fn cloned_texture_image_handle(&self, asset_id: AssetId) -> Option<Handle<Image>> {
        self.texture_image_handle(asset_id).cloned()
    }

    pub(crate) fn cloned_interactive_texture_metadata_handle(
        &self,
        asset_id: AssetId,
    ) -> Option<Handle<InteractiveTextureMetadataAsset>> {
        find_typed_dependency_handle(&self.interactive_texture_metadata_handles, asset_id).cloned()
    }

    pub(crate) fn scene_prefab_handle(
        &self,
        asset_id: AssetId,
    ) -> Option<&Handle<ScenePrefabAsset>> {
        find_typed_dependency_handle(&self.scene_prefab_handles, asset_id)
    }

    pub(crate) fn nested_ui_document_handle(
        &self,
        asset_id: AssetId,
    ) -> Option<&Handle<UiDocumentAsset>> {
        find_typed_dependency_handle(&self.nested_ui_document_handles, asset_id)
    }

    pub(crate) fn audio_source_handle(&self, asset_id: AssetId) -> Option<&Handle<AudioSource>> {
        find_typed_dependency_handle(&self.audio_source_handles, asset_id)
    }
}

fn find_typed_dependency_handle<T: Asset>(
    typed_dependency_handles: &[(AssetId, Handle<T>)],
    asset_id: AssetId,
) -> Option<&Handle<T>> {
    typed_dependency_handles
        .binary_search_by_key(&asset_id.0, |(candidate, _)| candidate.0)
        .ok()
        .map(|index| &typed_dependency_handles[index].1)
}
