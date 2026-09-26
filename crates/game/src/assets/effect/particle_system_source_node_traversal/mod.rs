//! Traverses repaired PSYS nodes by local element name and authored parameter label.

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

pub(super) fn particle_system_source_children_named<'a>(
    source_node: &'a OrderedSourceDocumentNode,
    required_local_name: &'a str,
) -> impl Iterator<Item = &'a OrderedSourceDocumentNode> {
    source_node.element_children().filter(move |child_node| {
        particle_system_source_local_name(&child_node.name) == required_local_name
    })
}

pub(super) fn particle_system_source_local_name(qualified_name: &str) -> &str {
    qualified_name.rsplit(':').next().unwrap_or(qualified_name)
}

pub(super) fn describe_particle_system_source_node(
    source_node: &OrderedSourceDocumentNode,
) -> String {
    format!(
        "{} handle={}",
        particle_system_source_local_name(&source_node.name),
        source_node.attribute("handle").unwrap_or("?"),
    )
}

pub(super) fn find_particle_system_parameter<'a>(
    source_node: &'a OrderedSourceDocumentNode,
    parameter_label: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    particle_system_source_children_named(source_node, "param").find(|parameter_node| {
        parameter_node
            .attribute("label")
            .is_some_and(|authored_label| authored_label.eq_ignore_ascii_case(parameter_label))
    })
}

pub(super) fn find_particle_system_parameter_by_any_label<'a>(
    source_node: &'a OrderedSourceDocumentNode,
    accepted_parameter_labels: &[&str],
) -> Option<&'a OrderedSourceDocumentNode> {
    particle_system_source_children_named(source_node, "param").find(|parameter_node| {
        parameter_node
            .attribute("label")
            .is_some_and(|authored_label| {
                accepted_parameter_labels
                    .iter()
                    .any(|accepted_label| authored_label.eq_ignore_ascii_case(accepted_label))
            })
    })
}
