use std::collections::BTreeMap;

use crate::source_document::ordered_source_document_types::OrderedSourceDocument;

use super::{BindError, IndexedElement, canonicalize_source_document_record_key};
use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

pub(super) fn build_precedence_resolved_source_record_index<'a>(
    documents: impl IntoIterator<Item = &'a OrderedSourceDocument>,
    additional_named_record_element_types: &[&str],
) -> Result<BTreeMap<String, IndexedElement<'a>>, BindError> {
    let mut records = BTreeMap::new();
    for document in documents {
        let root = IndexedElement {
            document,
            element: &document.root,
            root: true,
        };
        insert_precedence_resolved_source_record(
            &mut records,
            root,
            true,
            additional_named_record_element_types,
        )?;
        index_source_document_descendants(
            document,
            &document.root,
            &mut records,
            additional_named_record_element_types,
        )?;
    }
    Ok(records)
}

fn index_source_document_descendants<'a>(
    document: &'a OrderedSourceDocument,
    element: &'a OrderedSourceDocumentNode,
    records: &mut BTreeMap<String, IndexedElement<'a>>,
    additional_named_record_element_types: &[&str],
) -> Result<(), BindError> {
    for child in element.element_children() {
        let indexed = IndexedElement {
            document,
            element: child,
            root: false,
        };
        insert_precedence_resolved_source_record(
            records,
            indexed,
            false,
            additional_named_record_element_types,
        )?;
        index_source_document_descendants(
            document,
            child,
            records,
            additional_named_record_element_types,
        )?;
    }
    Ok(())
}

fn insert_precedence_resolved_source_record<'a>(
    records: &mut BTreeMap<String, IndexedElement<'a>>,
    indexed: IndexedElement<'a>,
    is_root: bool,
    additional_named_record_element_types: &[&str],
) -> Result<(), BindError> {
    let Some(key) = resolved_source_record_key(
        indexed.element,
        is_root,
        additional_named_record_element_types,
    )
    .or_else(|| {
        is_root.then(|| canonicalize_source_document_record_key(&indexed.document.path.key()))
    }) else {
        return Ok(());
    };
    // Documents arrive in resolved archive/entry precedence order. Blue Fang
    // catalogues intentionally redeclare named records (and mods depend on
    // doing so), therefore the last resolved definition is the authored
    // winner. Live lowering folds that decision into the typed catalogue.
    records.insert(key, indexed);
    Ok(())
}

fn resolved_source_record_key(
    element: &'_ OrderedSourceDocumentNode,
    is_root: bool,
    additional_named_record_element_types: &[&str],
) -> Option<String> {
    element
        .attribute_named_any(&["id", "typeName", "entityName", "binderType", "key"])
        .or_else(|| {
            (is_root
                || element
                    .attribute_named_any(&["extends", "base", "parentType", "inherit", "template"])
                    .is_some()
                || additional_named_record_element_types
                    .iter()
                    .any(|element_type| {
                        crate::source_document::source_document_semantic_name::source_document_names_are_semantically_equal(
                            element.name.as_str(),
                            element_type,
                        )
                    }))
            .then(|| element.attribute_named_any(&["name"]))
            .flatten()
        })
        .filter(|value| !value.trim().is_empty())
        .map(canonicalize_source_document_record_key)
}
