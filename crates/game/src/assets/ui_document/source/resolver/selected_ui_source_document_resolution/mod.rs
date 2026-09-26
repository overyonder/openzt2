//! Selected UI source resolution into role-owned and fragment-owned documents.

use std::collections::BTreeMap;

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentSpan,
};

use super::super::ui_source_document_gap::{
    UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
};

use super::{
    application_root_cursor_and_named_event_resolution::resolve_application_root_cursor_and_named_event_semantics,
    authored_biome_panel_composition::{
        attach_embedded_biome_panels_to_authored_shell_composition_point,
        parse_embedded_biome_panel_from_source_document,
    },
    authored_ui_source_skin_image_resolution::{
        apply_authored_selected_skin_replacements_to_source_documents,
        resolve_authored_file_and_directory_skin_image_selections,
    },
    gameplay_mode_ui_presentation_source_parsing::{
        parse_authored_interaction_cursors_from_gameplay_mode_document,
        parse_authored_placement_preview_from_gameplay_mode_document,
    },
    resolution_profile::SelectedUiSourceResolutionProfile,
    resolved_ui_document_types::ResolvedUiRoleAndFragmentDocuments,
    selected_ui_dependency_winner_resolution::{
        collect_ui_visual_image_paths_without_selected_dependency_winners,
        rewrite_ui_source_node_dependency_paths_to_selected_winners,
    },
    selected_ui_role_and_fragment_document_construction::construct_selected_ui_role_and_fragment_document_lowering_inputs,
    ui_source_document_parsing_and_normalization::parse_and_normalize_blue_fang_ui_source_document,
    ui_source_expansion_pack_availability_pruning::remove_ui_source_nodes_requiring_unavailable_expansion_packs,
    ui_source_template_collection_and_selection::{
        collect_named_ui_source_templates, collect_reusable_ui_list_row_template_names,
        remove_inline_ui_source_templates_lowered_as_reusable_row_documents,
    },
};

/// Resolves UI ownership entirely in live lowering. It removes losing paths,
/// collects the global Blue Fang template namespace, applies the selected
/// skin/theme replacements, assigns semantic role roots, and retains all other
/// authored layouts as path-addressed typed fragments.
pub(in crate::assets::ui_document::source) fn resolve_selected_ui_source_documents_to_role_and_fragment_lowering_inputs(
    selected_source_documents: &[&OrderedSourceDocument],
    resolution_profile: &SelectedUiSourceResolutionProfile,
) -> Result<ResolvedUiRoleAndFragmentDocuments, UiSourceDocumentGap> {
    let selected_source_document_winners_by_path = selected_source_documents
        .iter()
        .enumerate()
        .fold(BTreeMap::new(), |mut winners, (precedence, document)| {
            winners.insert(document.path.key(), (precedence, *document));
            winners
        });
    let mut parsed_ui_source_documents_by_path = BTreeMap::new();
    let mut selected_source_documents_in_resolution_order =
        selected_source_document_winners_by_path
            .into_values()
            .collect::<Vec<_>>();
    if resolution_profile.source_precedence.is_empty() {
        selected_source_documents_in_resolution_order.sort_by_key(|(input_order, _)| *input_order);
    } else {
        if let Some((_, document)) =
            selected_source_documents_in_resolution_order
                .iter()
                .find(|(_, document)| {
                    !resolution_profile
                        .source_precedence
                        .contains_key(&document.path.key())
                })
        {
            return Err(UiSourceDocumentGap {
                family: UiSourceDocumentFamily::Ui,
                kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                virtual_path: document.path.key(),
                span: OrderedSourceDocumentSpan::default(),
                message: "selected UI document has no planner source precedence".into(),
            });
        }
        selected_source_documents_in_resolution_order.sort_by_key(|(_, document)| {
            resolution_profile
                .source_precedence
                .get(&document.path.key())
                .copied()
                .expect("precedence coverage checked")
        });
    }
    let interaction_mode_path = resolution_profile
        .interaction_mode_path
        .as_deref()
        .map(|path| path.replace('\\', "/").to_ascii_lowercase());
    let interaction_cursors = interaction_mode_path
        .as_deref()
        .map(|path| {
            selected_source_documents_in_resolution_order
                .iter()
                .find_map(|(_, document)| (document.path.key() == path).then_some(*document))
                .ok_or_else(|| UiSourceDocumentGap {
                    family: UiSourceDocumentFamily::Ui,
                    kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                    virtual_path: path.to_owned(),
                    span: OrderedSourceDocumentSpan::default(),
                    message: "resolved core UI documents has no gameplay mode document".into(),
                })
                .and_then(parse_authored_interaction_cursors_from_gameplay_mode_document)
        })
        .transpose()?;
    let placement_preview = interaction_mode_path
        .as_deref()
        .map(|path| {
            selected_source_documents_in_resolution_order
                .iter()
                .find_map(|(_, document)| (document.path.key() == path).then_some(*document))
                .ok_or_else(|| UiSourceDocumentGap {
                    family: UiSourceDocumentFamily::Ui,
                    kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                    virtual_path: path.to_owned(),
                    span: OrderedSourceDocumentSpan::default(),
                    message: "resolved core UI documents has no gameplay mode document".into(),
                })
                .and_then(parse_authored_placement_preview_from_gameplay_mode_document)
        })
        .transpose()?;
    let mut parsed_ui_source_document_order = Vec::new();
    let mut authored_biome_panels = Vec::new();
    for (_, document) in selected_source_documents_in_resolution_order {
        let path = document.path.key();
        if interaction_mode_path.as_deref() == Some(path.as_str()) {
            continue;
        }
        if let Some(panel) = parse_embedded_biome_panel_from_source_document(document)? {
            authored_biome_panels.push(panel);
        }
        if path != "ztapp4.xml" && !path.starts_with("ui/") {
            continue;
        }
        match parse_and_normalize_blue_fang_ui_source_document(document) {
            Ok(mut source) => {
                source.diagnostics.clear();
                let path = source.path.key();
                if remove_ui_source_nodes_requiring_unavailable_expansion_packs(
                    &mut source.root,
                    &resolution_profile.available_xpacks,
                ) {
                    parsed_ui_source_document_order.push(path.clone());
                    parsed_ui_source_documents_by_path.insert(path, source);
                }
            }
            Err(gap) if gap.kind == UiSourceDocumentGapKind::WrongDocumentFamily => {}
            Err(gap) => return Err(gap),
        }
    }
    if !authored_biome_panels.is_empty() {
        let shell = parsed_ui_source_documents_by_path
            .get_mut("ui/layout/shell.xml")
            .ok_or_else(|| UiSourceDocumentGap {
                family: UiSourceDocumentFamily::Ui,
                kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                virtual_path: "ui/layout/shell.xml".into(),
                span: OrderedSourceDocumentSpan::default(),
                message: "canonical biome panels have no in-game shell owner".into(),
            })?;
        let attached = attach_embedded_biome_panels_to_authored_shell_composition_point(
            &mut shell.root,
            &mut authored_biome_panels,
        );
        if !attached {
            return Err(UiSourceDocumentGap {
                family: UiSourceDocumentFamily::Ui,
                kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
                virtual_path: shell.path.key(),
                span: shell.root.span,
                message: "in-game shell has no authored Biome Panel composition point".into(),
            });
        }
    }
    resolve_application_root_cursor_and_named_event_semantics(
        &mut parsed_ui_source_documents_by_path,
    )?;
    let image_selections = resolve_authored_file_and_directory_skin_image_selections(
        &parsed_ui_source_documents_by_path,
        &resolution_profile.available_assets,
    )?;
    apply_authored_selected_skin_replacements_to_source_documents(
        &mut parsed_ui_source_documents_by_path,
        &parsed_ui_source_document_order,
        resolution_profile.theme.as_deref(),
        &resolution_profile.resolved_dependencies.images,
    )?;
    let mut resolved_dependencies = resolution_profile.resolved_dependencies.clone();
    for source in parsed_ui_source_documents_by_path.values() {
        collect_ui_visual_image_paths_without_selected_dependency_winners(
            &source.path.key(),
            &source.root,
            &resolved_dependencies.images,
            &mut resolved_dependencies.absent_visual_images,
        );
    }
    for source in parsed_ui_source_documents_by_path.values_mut() {
        let owner = source.path.key();
        rewrite_ui_source_node_dependency_paths_to_selected_winners(
            &owner,
            &mut source.root,
            &resolved_dependencies,
        );
    }
    let mut templates = BTreeMap::new();
    for path in parsed_ui_source_document_order {
        if let Some(source) = parsed_ui_source_documents_by_path.get(&path) {
            collect_named_ui_source_templates(&source.root, &mut templates)?;
        }
    }
    remove_inline_ui_source_templates_lowered_as_reusable_row_documents(
        &mut parsed_ui_source_documents_by_path,
    );
    let row_template_names = collect_reusable_ui_list_row_template_names(
        &parsed_ui_source_documents_by_path,
        &templates,
    );

    construct_selected_ui_role_and_fragment_document_lowering_inputs(
        parsed_ui_source_documents_by_path,
        &templates,
        &row_template_names,
        &image_selections,
        interaction_cursors,
        placement_preview,
        resolved_dependencies,
        resolution_profile,
    )
}
