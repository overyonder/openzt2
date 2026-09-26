//! UI document assets and loading.

mod source;

mod ui_document_asset_dependency_loading;
mod ui_document_asset_loading_and_labeled_subasset_construction;
pub(crate) mod ui_document_asset_types_and_borrowing_queries;
pub(crate) mod ui_document_role_asset_path_selection;
mod ui_document_role_source_path_declarations;
mod ui_template_declaration_path_index_cache;

use bevy::prelude::*;

use ui_document_asset_loading_and_labeled_subasset_construction::UiDocumentAssetLoader;
use ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

#[derive(Default)]
pub(super) struct UiDocumentAssetPlugin;

impl Plugin for UiDocumentAssetPlugin {
    fn build(&self, application: &mut App) {
        application
            .init_asset::<UiDocumentAsset>()
            .init_asset_loader::<UiDocumentAssetLoader>();
    }
}
