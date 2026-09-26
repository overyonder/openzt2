use std::collections::{BTreeMap, BTreeSet};

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentSpan,
};

use super::{canonicalize_source_document_record_key, BindError, IndexedElement};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

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
    validate_resolved_source_record_inheritance(&records)?;
    Ok(records)
}

fn validate_resolved_source_record_inheritance(
    records: &BTreeMap<String, IndexedElement<'_>>,
) -> Result<(), BindError> {
    for start in records.keys() {
        let mut current = start;
        let mut visiting = BTreeSet::new();
        loop {
            if !visiting.insert(current.as_str()) {
                let indexed = records.get(current).ok_or_else(|| BindError {
                    virtual_path: "<world-catalogue-index>".to_owned(),
                    span: OrderedSourceDocumentSpan::default(),
                    message: format!("missing indexed inheritance key {current}"),
                })?;
                return Err(BindError::at(
                    indexed.document,
                    indexed.element,
                    format!("source inheritance cycle at {current}"),
                ));
            }
            let indexed = records.get(current).ok_or_else(|| BindError {
                virtual_path: "<world-catalogue-index>".to_owned(),
                span: OrderedSourceDocumentSpan::default(),
                message: format!("missing indexed inheritance key {current}"),
            })?;
            let Some(base) =
                indexed
                    .element
                    .attribute_named_any(&["extends", "base", "parentType", "inherit"])
            else {
                break;
            };
            let base = canonicalize_source_document_record_key(base);
            let Some((key, _)) = records.get_key_value(&base) else {
                return Err(BindError::at(
                    indexed.document,
                    indexed.element,
                    format!("unresolved source base {base}"),
                ));
            };
            current = key;
        }
    }
    Ok(())
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
                        crate::assets::source_document::source_document_semantic_name::source_document_names_are_semantically_equal(
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
