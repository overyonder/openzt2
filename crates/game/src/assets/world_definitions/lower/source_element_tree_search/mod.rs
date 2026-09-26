use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::assets::source_document::resolved_source_record_index::RecordView;
use crate::assets::source_document::source_document_semantic_name::source_document_names_are_semantically_equal;

pub(super) fn find_descendant<'a>(
    element: &'a OrderedSourceDocumentNode,
    name: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    element.element_children().find_map(|child| {
        source_document_names_are_semantically_equal(child.name.as_str(), name)
            .then_some(child)
            .or_else(|| find_descendant(child, name))
    })
}

pub(super) fn find_presentation_component<'a>(
    element: &'a OrderedSourceDocumentNode,
) -> Option<&'a OrderedSourceDocumentNode> {
    element.element_children().find_map(|child| {
        let supported = source_document_names_are_semantically_equal(
            child.name.as_str(),
            "BFSimpleLODComponent",
        ) || source_document_names_are_semantically_equal(
            child.name.as_str(),
            "BFSceneGraphComponent",
        ) || source_document_names_are_semantically_equal(
            child.name.as_str(),
            "BFRSceneGraphComponent",
        ) || source_document_names_are_semantically_equal(
            child.name.as_str(),
            "BFActorComponent",
        );
        (supported
            && child
                .attribute_named_any(&["modelfile", "actorfile"])
                .is_some_and(|value| !value.trim().is_empty()))
        .then_some(child)
        .or_else(|| find_presentation_component(child))
    })
}

pub(super) fn find_descendant_with_attribute<'a>(
    element: &'a OrderedSourceDocumentNode,
    name: &str,
    attribute: &str,
    value: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    element.element_children().find_map(|child| {
        (source_document_names_are_semantically_equal(child.name.as_str(), name)
            && child
                .attribute_named_any(&[attribute])
                .is_some_and(|candidate| {
                    source_document_names_are_semantically_equal(candidate, value)
                }))
        .then_some(child)
        .or_else(|| find_descendant_with_attribute(child, name, attribute, value))
    })
}

pub(super) fn find_descendant_named_with_nonempty_attribute<'a>(
    element: &'a OrderedSourceDocumentNode,
    name: &str,
    attribute: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    element.element_children().find_map(|child| {
        (source_document_names_are_semantically_equal(child.name.as_str(), name)
            && child
                .attribute_named_any(&[attribute])
                .is_some_and(|value| !value.trim().is_empty()))
        .then_some(child)
        .or_else(|| find_descendant_named_with_nonempty_attribute(child, name, attribute))
    })
}

/// Finds matching components in the binder and its ancestors, most-specific first.
pub(super) fn authored_type_family_components<'index, 'document>(
    record: &RecordView<'index, 'document>,
    component: &str,
) -> Vec<&'document OrderedSourceDocumentNode> {
    let direct = find_descendant(record.source_document_element(), component);
    let mut components = direct.into_iter().collect::<Vec<_>>();
    components.extend(
        record
            .type_tokens()
            .into_iter()
            .rev()
            .filter(|family_key| {
                !source_document_names_are_semantically_equal(family_key, record.key)
            })
            .filter_map(|family_key| record.find_resolved_source_record_by_reference(&family_key))
            .filter_map(|family_record| {
                find_descendant(family_record.source_document_element(), component)
            }),
    );
    components
}

pub(super) fn authored_type_family_elements_named<'index, 'document>(
    record: &RecordView<'index, 'document>,
    element_name: &str,
) -> Vec<&'document OrderedSourceDocumentNode> {
    fn append_named_descendants<'document>(
        element: &'document OrderedSourceDocumentNode,
        element_name: &str,
        output: &mut Vec<&'document OrderedSourceDocumentNode>,
    ) {
        for child in element.element_children() {
            if source_document_names_are_semantically_equal(child.name.as_str(), element_name) {
                output.push(child);
            }
            append_named_descendants(child, element_name, output);
        }
    }

    let mut elements = Vec::new();
    append_named_descendants(
        record.source_document_element(),
        element_name,
        &mut elements,
    );
    for family_record in record
        .type_tokens()
        .into_iter()
        .rev()
        .filter(|family_key| !source_document_names_are_semantically_equal(family_key, record.key))
        .filter_map(|family_key| record.find_resolved_source_record_by_reference(&family_key))
    {
        append_named_descendants(
            family_record.source_document_element(),
            element_name,
            &mut elements,
        );
    }
    elements
}

pub(super) fn authored_type_family_component_attribute<'index, 'document>(
    record: &RecordView<'index, 'document>,
    component: &str,
    attribute: &str,
) -> Option<&'document str> {
    authored_type_family_components(record, component)
        .into_iter()
        .find_map(|component| component.attribute_named_any(&[attribute]))
}

/// Selects the first authored family variant that directly owns a complete
/// source attribute.
///
/// Original entity families encode variants by repeating the complete nested
/// `<types>` path instead of using `extends`. This query exists only while the
/// world-definition lowerer borrows the precedence-winning source records.
pub(super) fn find_first_authored_family_variant_with_nonempty_attribute<'index, 'document>(
    record: &RecordView<'index, 'document>,
    resolved_source_records: &[RecordView<'index, 'document>],
    component: &str,
    attribute: &str,
) -> Option<RecordView<'index, 'document>> {
    resolved_source_records
        .iter()
        .copied()
        .filter(|candidate| {
            candidate.key != record.key
                && (candidate.has_type_token(record.key)
                    || candidate
                        .descendant_named("BFAIEntityDataShared")
                        .and_then(|shared| shared.attribute_named_any(&["s_Species", "species"]))
                        .is_some_and(|species| {
                            source_document_names_are_semantically_equal(species, record.key)
                        }))
                && find_descendant(candidate.source_document_element(), component)
                    .and_then(|element| element.attribute_named_any(&[attribute]))
                    .is_some_and(|value| !value.trim().is_empty())
        })
        .min_by(|left, right| left.key.cmp(right.key))
}
