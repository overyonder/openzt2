use super::resolved_source_record_index::RecordView;
use super::source_document_semantic_name::source_document_names_are_semantically_equal;

pub fn find_authored_species_purchase_family_root<'index, 'document>(
    record: &RecordView<'index, 'document>,
    resolved_source_records: impl IntoIterator<Item = RecordView<'index, 'document>>,
) -> Option<RecordView<'index, 'document>> {
    std::iter::once(*record)
        .chain(resolved_source_records)
        .find(|candidate| {
            source_record_is_species_purchase_family_root(candidate)
                && (candidate.key == record.key
                    || record.has_type_token(candidate.key)
                    || record
                        .descendant_named("BFAIEntityDataShared")
                        .and_then(|shared| shared.attribute_named_any(&["s_Species"]))
                        .is_some_and(|value| {
                            source_document_names_are_semantically_equal(value, candidate.key)
                        }))
        })
}

fn source_record_is_species_purchase_family_root(record: &RecordView<'_, '_>) -> bool {
    record.has_type_token("animal")
        && record
            .source_document_element()
            .attribute_named_any(&["abstract"])
            .is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes"
                )
            })
        && record.descendant_named("UIToggleButton").is_some()
        && record
            .descendant_named("BFAIEntityDataShared")
            .and_then(|element| element.attribute_named_any(&["s_Species"]))
            .is_some_and(|value| source_document_names_are_semantically_equal(value, record.key))
}
