//! Authored money-text formatting normalization.

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocumentAttribute, OrderedSourceDocumentChild, OrderedSourceDocumentChildren,
    OrderedSourceDocumentNode, OrderedSourceDocumentSpan,
};

pub(super) fn normalize_authored_money_text_widget(
    attributes: &mut Vec<OrderedSourceDocumentAttribute>,
    children: &mut OrderedSourceDocumentChildren,
    span: OrderedSourceDocumentSpan,
) {
    let authored_attribute = |attribute_name: &str| {
        attributes
            .iter()
            .find(|attribute| attribute.name() == attribute_name)
            .map(|attribute| attribute.value().to_owned())
    };
    let positive_format = authored_attribute("posformat").unwrap_or_default();
    let negative_format = authored_attribute("negformat").unwrap_or_default();
    let omit_cents = authored_boolean_value(authored_attribute("nocents").as_deref());
    let omit_minus = authored_boolean_value(authored_attribute("nominus").as_deref());
    let canonical_money_format = format!(
        "openzt2:money:no-cents={};no-minus={};positive={};negative={}",
        u8::from(omit_cents),
        u8::from(omit_minus),
        positive_format,
        negative_format
    );

    attributes.retain(|attribute| {
        !matches!(
            attribute.name(),
            "posformat" | "negformat" | "nocents" | "nominus"
        )
    });
    let mut normalized_children = children.items().to_vec();
    normalized_children.push(OrderedSourceDocumentChild::Element(Box::new(
        OrderedSourceDocumentNode::new_synthetic_ordered_source_document_node(
            "text".into(),
            vec![
                OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
                    "format",
                    canonical_money_format,
                ),
            ],
            OrderedSourceDocumentChildren::default(),
            span,
        ),
    )));
    *children = OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
        normalized_children,
    );
}

fn authored_boolean_value(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}
