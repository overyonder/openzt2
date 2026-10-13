//! The behavior types an authored record answers to as a task subject or target.

use std::collections::BTreeSet;

use openzt2_game_data::AssetId;

use super::{
    ordered_source_document_types::OrderedSourceDocumentNode,
    resolved_source_record_index::RecordView,
    source_document_semantic_name::source_document_names_are_semantically_equal,
};

/// A record's type ancestry, its own key, and every `b_` flag its resolved
/// family leaves true. Later false values remove inherited true flags.
pub fn authored_behavior_type_identifiers(record: &RecordView<'_, '_>) -> Vec<AssetId> {
    fn descendants_named<'a>(
        element: &'a OrderedSourceDocumentNode,
        wanted: &str,
        output: &mut Vec<&'a OrderedSourceDocumentNode>,
    ) {
        for child in element.element_children() {
            if source_document_names_are_semantically_equal(child.name.as_str(), wanted) {
                output.push(child);
            }
            descendants_named(child, wanted, output);
        }
    }

    let ancestry = record.type_tokens();
    let mut types = ancestry
        .iter()
        .map(|name| AssetId::from_key(&name.to_ascii_lowercase()))
        .collect::<BTreeSet<_>>();
    types.insert(AssetId::from_key(&record.key.to_ascii_lowercase()));
    let family = ancestry
        .iter()
        .filter(|name| !source_document_names_are_semantically_equal(name, record.key))
        .filter_map(|name| record.find_resolved_source_record_by_reference(name))
        .chain(std::iter::once(*record));
    for member in family {
        let mut groups = Vec::new();
        for group in ["BFAIEntityDataShared", "BFAIEntityDataInstance"] {
            descendants_named(member.source_document_element(), group, &mut groups);
        }
        for data in groups {
            for (name, value) in data.attributes() {
                if !name.to_ascii_lowercase().starts_with("b_") {
                    continue;
                }
                let id = AssetId::from_key(&name.to_ascii_lowercase());
                if matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes"
                ) {
                    types.insert(id);
                } else {
                    types.remove(&id);
                }
            }
        }
    }
    types.into_iter().collect()
}
