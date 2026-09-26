//! Selected UI source resolution, canonical result selection, and lowering.

use std::collections::BTreeMap;

use openzt2_game_data::{
    ui_document::document::{UiDocument, UiDocumentRole},
    AssetId,
};

use crate::assets::source_document::{
    document_semantics::{classify_source_document, SourceDocumentKind},
    ordered_source_document_types::OrderedSourceDocument,
    path::AssetPath,
};

use super::{
    lower::authored_ui_document_lowering::{
        self as ui_document_lowering, UiResolvedDependencies, ADOPTION_SLOT_ROW_DOCUMENT,
    },
    resolver::{
        resolution_profile::SelectedUiSourceResolutionProfile,
        selected_ui_source_document_resolution::resolve_selected_ui_source_documents_to_role_and_fragment_lowering_inputs,
    },
    ui_source_document_gap::{
        UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
    },
};

pub(super) fn lower_selected_ui_source_documents_to_canonical_result_documents(
    source_documents: &[OrderedSourceDocument],
    resolved_dependencies: UiResolvedDependencies,
    primary_source_path: &str,
) -> Result<Vec<UiDocument>, UiSourceDocumentGap> {
    let mut resolution_profile =
        SelectedUiSourceResolutionProfile::for_primary_source_path(primary_source_path);
    resolution_profile.available_assets = resolved_dependencies.images.values().cloned().collect();
    resolution_profile.resolved_dependencies = resolved_dependencies;
    for (role, source_path) in &resolution_profile.role_paths {
        resolution_profile.resolved_dependencies.documents.insert(
            format!("*\0{source_path}"),
            format!("ui/role/{}", role.stable_key()),
        );
    }

    let ui_and_embedded_biome_panel_source_documents = source_documents
        .iter()
        .filter(|document| {
            classify_source_document(document) == SourceDocumentKind::Ui
                || document.root.name.eq_ignore_ascii_case("BFGBiome")
        })
        .collect::<Vec<_>>();
    resolution_profile.source_precedence = ui_and_embedded_biome_panel_source_documents
        .iter()
        .enumerate()
        .map(|(index, document)| {
            (
                document.path.key(),
                (u32::try_from(index).unwrap_or(u32::MAX), 0),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let has_role_documents = !resolution_profile.role_paths.is_empty();
    let primary_fragment_id = AssetId::from_virtual_path(&format!(
        "ui/fragment/{}",
        AssetPath::new(primary_source_path).key()
    ));
    let resolved_documents =
        resolve_selected_ui_source_documents_to_role_and_fragment_lowering_inputs(
            &ui_and_embedded_biome_panel_source_documents,
            &resolution_profile,
        )?;

    resolved_documents
        .roles
        .into_iter()
        .chain(resolved_documents.fragments)
        .filter(|document| {
            let reusable_row_document = (document.virtual_path.starts_with("ui/template/")
                && document.virtual_path.ends_with("#row"))
                || document.virtual_path == ADOPTION_SLOT_ROW_DOCUMENT;
            if has_role_documents {
                document.role != UiDocumentRole::Fragment || reusable_row_document
            } else {
                document.id == primary_fragment_id || reusable_row_document
            }
        })
        .map(|document| {
            ui_document_lowering::lower_ui_document(&document).map_err(|error| {
                UiSourceDocumentGap {
                    family: UiSourceDocumentFamily::Ui,
                    kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                    virtual_path: document.virtual_path,
                    span: document.source.root.span,
                    message: error.to_string(),
                }
            })
        })
        .collect()
}
