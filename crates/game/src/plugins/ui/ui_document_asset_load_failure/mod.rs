use bevy::prelude::*;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

/// One requested authored UI document whose asset loader reported failure
/// before it could be projected for its lifecycle owner.
#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub(crate) struct UiDocumentAssetLoadFailed {
    pub(crate) owner: Entity,
    pub(crate) document: Handle<UiDocumentAsset>,
}
