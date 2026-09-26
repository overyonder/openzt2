//! Typed Bevy dependency loading for one UI document.

use std::collections::BTreeMap;

use bevy::{asset::LoadContext, prelude::*};
use openzt2_game_data::{
    ui_document::document::{UiDependencyKind, UiDocument},
    AssetId,
};

use super::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

pub(super) fn create_ui_document_asset_and_load_typed_dependencies(
    canonical_ui_document: UiDocument,
    same_source_fragment_handles: &BTreeMap<AssetId, Handle<UiDocumentAsset>>,
    load_context: &mut LoadContext<'_>,
) -> UiDocumentAsset {
    let mut texture_image_handles = Vec::new();
    let mut interactive_texture_metadata_handles = Vec::new();
    let mut scene_prefab_handles = Vec::new();
    let mut nested_ui_document_handles = Vec::new();
    let mut audio_source_handles = Vec::new();
    for dependency in &canonical_ui_document.dependencies {
        match dependency.kind {
            UiDependencyKind::Texture => {
                texture_image_handles.push((
                    dependency.id,
                    load_context.load(
                        crate::assets::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image(
                            &dependency.path,
                        ),
                    ),
                ));
            }
            UiDependencyKind::InteractiveTexture => {
                texture_image_handles.push((
                    dependency.id,
                    load_context.load(
                        crate::assets::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image(
                            &dependency.path,
                        ),
                    ),
                ));
                interactive_texture_metadata_handles
                    .push((dependency.id, load_context.load(dependency.path.clone())));
            }
            UiDependencyKind::Scene => {
                scene_prefab_handles
                    .push((dependency.id, load_context.load(dependency.path.clone())));
            }
            UiDependencyKind::Document => {
                nested_ui_document_handles.push((
                    dependency.id,
                    same_source_fragment_handles
                        .get(&dependency.id)
                        .cloned()
                        .unwrap_or_else(|| {
                            load_context.load(
                                dependency
                                    .path
                                    .strip_prefix("ui/fragment/")
                                    .unwrap_or(&dependency.path)
                                    .to_owned(),
                            )
                        }),
                ));
            }
            UiDependencyKind::Audio => {
                audio_source_handles
                    .push((dependency.id, load_context.load(dependency.path.clone())));
            }
        }
    }
    texture_image_handles.sort_unstable_by_key(|(asset_id, _)| asset_id.0);
    interactive_texture_metadata_handles.sort_unstable_by_key(|(asset_id, _)| asset_id.0);
    scene_prefab_handles.sort_unstable_by_key(|(asset_id, _)| asset_id.0);
    nested_ui_document_handles.sort_unstable_by_key(|(asset_id, _)| asset_id.0);
    audio_source_handles.sort_unstable_by_key(|(asset_id, _)| asset_id.0);
    UiDocumentAsset {
        canonical_ui_document,
        texture_image_handles: texture_image_handles.into_boxed_slice(),
        interactive_texture_metadata_handles: interactive_texture_metadata_handles
            .into_boxed_slice(),
        scene_prefab_handles: scene_prefab_handles.into_boxed_slice(),
        nested_ui_document_handles: nested_ui_document_handles.into_boxed_slice(),
        audio_source_handles: audio_source_handles.into_boxed_slice(),
    }
}
