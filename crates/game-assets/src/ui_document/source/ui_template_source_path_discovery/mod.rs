//! Discovery of UI template source paths from the winning archive overlay.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use crate::{
    asset_source::AssetArchives,
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        ordered_source_document_types::{OrderedSourceDocument, OrderedSourceDocumentNode},
        path::AssetPath,
    },
};

pub(in crate::ui_document) fn collect_referenced_ui_template_source_paths(
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

pub(in crate::ui_document) fn read_ui_template_declarations(
    asset_archives: &AssetArchives,
    path: &Path,
) -> Vec<String> {
    let Some(document) = asset_archives.read_source(path).ok().and_then(|bytes| {
        parse_blue_fang_source_document(AssetPath::new(path.to_string_lossy()), &bytes).ok()
    }) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    collect_declared_ui_template_names_from_source_document_node(&document.root, &mut names);
    names
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
