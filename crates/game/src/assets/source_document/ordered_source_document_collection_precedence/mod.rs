//! Precedence ordering for one transient collection of parsed source documents.

use std::collections::BTreeMap;

use super::ordered_source_document_types::OrderedSourceDocument;

pub(in crate::assets) fn reorder_ordered_source_documents_by_requested_normalized_path_keys_and_last_definition_precedence(
    source_documents: &mut Vec<OrderedSourceDocument>,
    requested_normalized_path_keys: &[String],
) {
    let mut source_documents_by_normalized_path_key = std::mem::take(source_documents)
        .into_iter()
        .map(|source_document| (source_document.path.key(), source_document))
        .collect::<BTreeMap<_, _>>();
    source_documents.extend(requested_normalized_path_keys.iter().filter_map(
        |normalized_path_key| source_documents_by_normalized_path_key.remove(normalized_path_key),
    ));
    source_documents.extend(source_documents_by_normalized_path_key.into_values());
}
