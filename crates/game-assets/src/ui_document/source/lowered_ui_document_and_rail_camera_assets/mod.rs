//! Canonical assets produced by one Blue Fang UI source lowering operation.

use openzt2_game_data::{scene_prefab::ScenePrefabDocument, ui_document::document::UiDocument};

pub(in crate::ui_document) struct LoweredUiDocumentAndRailCameraAssets {
    pub(in crate::ui_document) documents: Vec<UiDocument>,
    pub(in crate::ui_document) fame_stars:
        Option<super::super::ui_document_asset_types_and_borrowing_queries::FameStarDisplay>,
    pub(in crate::ui_document) rail_cameras: Vec<(String, ScenePrefabDocument)>,
}
