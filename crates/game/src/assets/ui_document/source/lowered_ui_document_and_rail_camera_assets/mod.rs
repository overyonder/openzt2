//! Canonical assets produced by one Blue Fang UI source lowering operation.

use openzt2_game_data::{scene_prefab::ScenePrefabDocument, ui_document::document::UiDocument};

pub(in crate::assets::ui_document) struct LoweredUiDocumentAndRailCameraAssets {
    pub(in crate::assets::ui_document) documents: Vec<UiDocument>,
    pub(in crate::assets::ui_document) rail_cameras: Vec<(String, ScenePrefabDocument)>,
}
