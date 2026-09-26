use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::assets::source_document::resolved_source_record_index::RecordView;
use crate::assets::source_document::source_document_semantic_name::source_document_names_are_semantically_equal;

pub(super) fn find_inherited_animal_shared_attribute<'a>(
    record: &RecordView<'_, 'a>,
    attribute: &str,
) -> Option<&'a str> {
    record
        .descendant_named("BFAIEntityDataShared")
        .and_then(|shared| shared.attribute_named_any(&[attribute]))
        .or_else(|| {
            record
                .source_document_element()
                .attribute_named_any(&["extends", "base", "parentType", "inherit"])
                .and_then(|base| record.find_resolved_source_record_by_reference(base))
                .and_then(|base| find_inherited_animal_shared_attribute(&base, attribute))
        })
}

pub(super) fn find_all_semantically_named_source_descendants<'a>(
    element: &'a OrderedSourceDocumentNode,
    wanted: &str,
) -> Vec<&'a OrderedSourceDocumentNode> {
    fn append_semantically_named_source_descendants<'a>(
        element: &'a OrderedSourceDocumentNode,
        wanted: &str,
        output: &mut Vec<&'a OrderedSourceDocumentNode>,
    ) {
        for child in element.element_children() {
            if source_document_names_are_semantically_equal(child.name.as_str(), wanted) {
                output.push(child);
            }
            append_semantically_named_source_descendants(child, wanted, output);
        }
    }

    let mut output = Vec::new();
    append_semantically_named_source_descendants(element, wanted, &mut output);
    output
}

pub(super) fn find_first_semantically_named_source_descendant<'a>(
    element: &'a OrderedSourceDocumentNode,
    wanted: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    element.element_children().find_map(|child| {
        source_document_names_are_semantically_equal(child.name.as_str(), wanted)
            .then_some(child)
            .or_else(|| find_first_semantically_named_source_descendant(child, wanted))
    })
}

pub(super) fn authored_optional_boolean_is_true(value: Option<&str>) -> bool {
    value.is_some_and(authored_boolean_is_true)
}

pub(super) fn authored_boolean_is_true(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes"
    )
}
