//! Blue Fang UI source resolution and semantic lowering.

mod lower;
mod lowered_ui_document_and_rail_camera_assets;
mod rail_camera;
mod resolver;
mod selected_ui_source_dependency_winner_planning;
mod selected_ui_source_document_canonical_result_lowering;
mod selected_ui_source_document_loading;
mod ui_source_document_gap;
pub(in crate::assets::ui_document) mod ui_template_source_path_discovery;

use std::{collections::BTreeMap, io};

use bevy::asset::LoadContext;

use lowered_ui_document_and_rail_camera_assets::LoweredUiDocumentAndRailCameraAssets;
use rail_camera::document_lowering::lower_rail_camera_documents_to_scene_prefab_documents;
use selected_ui_source_document_canonical_result_lowering::lower_selected_ui_source_documents_to_canonical_result_documents;
use selected_ui_source_document_loading::load_primary_supporting_and_referenced_ui_source_documents;

/// Lowers one selected UI source document.  The loader owns the resulting
/// typed asset; source dependencies are registered by that asset rather than
/// being retained in a document-set registry.
pub(crate) async fn lower(
    source_path: &str,
    bytes: &[u8],
    archives: &crate::asset_source::AssetArchives,
    template_paths: &BTreeMap<String, String>,
    context: &mut LoadContext<'_>,
) -> io::Result<LoweredUiDocumentAndRailCameraAssets> {
    let documents = load_primary_supporting_and_referenced_ui_source_documents(
        source_path,
        bytes,
        archives,
        template_paths,
        context,
    )
    .await?;
    let dependencies = selected_ui_source_dependency_winner_planning::
        plan_selected_ui_source_dependency_winners_and_image_dimensions(&documents, archives);
    let rail_cameras = lower_rail_camera_documents_to_scene_prefab_documents(&documents)?;
    let documents = lower_selected_ui_source_documents_to_canonical_result_documents(
        &documents,
        dependencies,
        source_path,
    )
    .map_err(|gap| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {}", gap.virtual_path, gap.message),
        )
    })?;
    Ok(LoweredUiDocumentAndRailCameraAssets {
        documents,
        rail_cameras,
    })
}
