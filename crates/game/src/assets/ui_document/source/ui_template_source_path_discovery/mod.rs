//! Discovery of UI template source paths from the winning archive overlay.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    asset_source::AssetArchives,
    assets::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        ordered_source_document_types::{OrderedSourceDocument, OrderedSourceDocumentNode},
        path::AssetPath,
    },
};

pub(in crate::assets::ui_document) fn collect_referenced_ui_template_source_paths(
    source_documents: &[OrderedSourceDocument],
    ui_template_declaration_source_paths: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut referenced_ui_template_names = BTreeSet::new();

    for source_document in source_documents {
        if let Some(ui_template_name) = source_document.root.attribute("template") {
            referenced_ui_template_names.insert(ui_template_name.trim().to_ascii_lowercase());
        }
        if source_document
            .root
            .name
            .eq_ignore_ascii_case("ZTUITypeList")
            || source_document
                .root
                .name
                .eq_ignore_ascii_case("ZTUIContextList")
        {
            referenced_ui_template_names.insert("purchaseicon".to_owned());
        }
        source_document
            .root
            .element_children()
            .for_each(|source_node| {
                collect_referenced_ui_template_names_from_source_document_node(
                    source_node,
                    &mut referenced_ui_template_names,
                );
            });
    }

    referenced_ui_template_names
        .into_iter()
        .filter_map(|ui_template_name| {
            ui_template_declaration_source_paths
                .get(&ui_template_name)
                .cloned()
        })
        .collect()
}

pub(in crate::assets::ui_document) fn build_ui_template_declaration_source_path_index(
    asset_archives: &AssetArchives,
) -> BTreeMap<String, String> {
    let candidate_ui_template_source_paths = asset_archives
        .resolved_paths()
        .1
        .iter()
        .map(|source_path| {
            z2f::paths::normalize_asset_path_for_case_insensitive_archive_lookup(source_path)
        })
        .filter(|source_path| {
            source_path.starts_with("ui/template")
                && source_path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
        })
        .collect::<Vec<_>>();

    let mut ui_template_declaration_source_paths = BTreeMap::new();
    for source_path in candidate_ui_template_source_paths {
        let source_path_string = source_path.to_string_lossy().replace('\\', "/");
        let Some(source_document) =
            asset_archives
                .read_source(&source_path)
                .ok()
                .and_then(|source_bytes| {
                    parse_blue_fang_source_document(
                        AssetPath::new(&source_path_string),
                        &source_bytes,
                    )
                    .ok()
                })
        else {
            continue;
        };

        let mut declared_ui_template_names = source_document
            .root
            .attribute("templateName")
            .map(|ui_template_name| vec![ui_template_name.trim().to_ascii_lowercase()])
            .unwrap_or_default();
        source_document
            .root
            .element_children()
            .for_each(|source_node| {
                collect_declared_ui_template_names_from_source_document_node(
                    source_node,
                    &mut declared_ui_template_names,
                );
            });

        for declared_ui_template_name in declared_ui_template_names {
            ui_template_declaration_source_paths
                .insert(declared_ui_template_name, source_path_string.clone());
        }
    }

    ui_template_declaration_source_paths
}

fn collect_referenced_ui_template_names_from_source_document_node(
    source_node: &OrderedSourceDocumentNode,
    referenced_ui_template_names: &mut BTreeSet<String>,
) {
    if let Some(ui_template_name) = source_node.attribute("template") {
        referenced_ui_template_names.insert(ui_template_name.trim().to_ascii_lowercase());
    }
    if source_node.name.eq_ignore_ascii_case("ZTUITypeList")
        || source_node.name.eq_ignore_ascii_case("ZTUIContextList")
    {
        referenced_ui_template_names.insert("purchaseicon".to_owned());
    }
    source_node
        .element_children()
        .for_each(|child_source_node| {
            collect_referenced_ui_template_names_from_source_document_node(
                child_source_node,
                referenced_ui_template_names,
            );
        });
}

fn collect_declared_ui_template_names_from_source_document_node(
    source_node: &OrderedSourceDocumentNode,
    declared_ui_template_names: &mut Vec<String>,
) {
    if let Some(ui_template_name) = source_node.attribute("templateName") {
        declared_ui_template_names.push(ui_template_name.trim().to_ascii_lowercase());
    }
    source_node
        .element_children()
        .for_each(|child_source_node| {
            collect_declared_ui_template_names_from_source_document_node(
                child_source_node,
                declared_ui_template_names,
            );
        });
}
