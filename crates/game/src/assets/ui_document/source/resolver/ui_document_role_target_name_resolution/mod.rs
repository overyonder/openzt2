//! Unique authored node-name resolution to semantic UI document roles.

use std::collections::BTreeMap;

use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::assets::source_document::{
    ordered_source_document_types::OrderedSourceDocumentSpan,
    ui::{model::SourceUiNode, parser::SourceUiDocument},
};

use super::{
    super::ui_source_document_gap::{
        UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
    },
    resolution_profile::SelectedUiSourceResolutionProfile,
    ui_document_role_specialization_and_composition::specialize_authored_ui_source_document_for_role,
};

pub(super) fn collect_unique_authored_node_name_targets_by_ui_document_role(
    sources: &BTreeMap<String, SourceUiDocument>,
    profile: &SelectedUiSourceResolutionProfile,
) -> Result<BTreeMap<String, UiDocumentRole>, UiSourceDocumentGap> {
    fn collect_names(
        node: &SourceUiNode,
        role: UiDocumentRole,
        candidates: &mut BTreeMap<String, Option<UiDocumentRole>>,
    ) {
        if let Some(name) = node.name.as_deref() {
            let name = name.trim().to_ascii_lowercase();
            candidates
                .entry(name)
                .and_modify(|candidate| {
                    if candidate.is_some_and(|candidate| candidate != role) {
                        *candidate = None;
                    }
                })
                .or_insert(Some(role));
        }
        node.children
            .iter()
            .for_each(|child| collect_names(child, role, candidates));
    }

    let mut candidates: BTreeMap<String, Option<UiDocumentRole>> = BTreeMap::new();
    for (role, authored_surface_name) in &profile.role_surfaces {
        let name = authored_surface_name.trim().to_ascii_lowercase();
        candidates
            .entry(name)
            .and_modify(|candidate| {
                if candidate.is_some_and(|candidate| candidate != *role) {
                    *candidate = None;
                }
            })
            .or_insert(Some(*role));
    }
    for (role, authored_path) in &profile.role_paths {
        let path = authored_path.replace('\\', "/").to_ascii_lowercase();
        let source = sources.get(&path).ok_or_else(|| UiSourceDocumentGap {
            family: UiSourceDocumentFamily::Ui,
            kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
            virtual_path: path.clone(),
            span: OrderedSourceDocumentSpan::default(),
            message: format!("resolved core UI documents has no source document for role {role:?}"),
        })?;
        let source = specialize_authored_ui_source_document_for_role(source, *role)?;
        collect_names(&source.root, *role, &mut candidates);
    }
    Ok(candidates
        .into_iter()
        .filter_map(|(name, role)| role.map(|role| (name, role)))
        .collect())
}
