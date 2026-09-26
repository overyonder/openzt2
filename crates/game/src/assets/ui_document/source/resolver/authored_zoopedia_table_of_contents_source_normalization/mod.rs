//! Authored Zoopedia table-of-contents collection normalization.

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocumentAttribute, OrderedSourceDocumentChild, OrderedSourceDocumentChildren,
    OrderedSourceDocumentNode,
};

pub(super) fn normalize_authored_zoopedia_table_of_contents_collection(
    node: &mut OrderedSourceDocumentNode,
) {
    if node.name != "ZTZoopediaTOC" {
        return;
    }

    let mut row_template = node.children.items().iter().find_map(|child| match child {
        OrderedSourceDocumentChild::Element(entry) if entry.name == "entry" => entry
            .children
            .items()
            .iter()
            .find_map(|entry_child| match entry_child {
                OrderedSourceDocumentChild::Element(template) => Some(template.as_ref().clone()),
                OrderedSourceDocumentChild::Text(_)
                | OrderedSourceDocumentChild::Comment
                | OrderedSourceDocumentChild::ProcessingInstruction => None,
            }),
        _ => None,
    });
    if let Some(row_template) = &mut row_template {
        row_template.attributes.push(
            OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
                "templateName",
                "openzt2_zoopedia_toc_entry",
            ),
        );
    }

    node.name = "UIListBox".into();
    node.attributes.extend([
        OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
            "openzt2ListSource",
            "zoopediaTableOfContents",
        ),
        OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
            "openzt2RowTemplate",
            "openzt2_zoopedia_toc_entry",
        ),
    ]);
    let mut normalized_children = node
        .children
        .items()
        .iter()
        .filter(
            |child| !matches!(child, OrderedSourceDocumentChild::Element(child) if child.name == "entry"),
        )
        .cloned()
        .collect::<Vec<_>>();
    if let Some(row_template) = row_template {
        normalized_children.push(OrderedSourceDocumentChild::Element(Box::new(row_template)));
    }
    node.children = OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
        normalized_children,
    );
}
