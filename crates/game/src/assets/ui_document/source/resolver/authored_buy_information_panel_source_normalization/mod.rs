//! Authored buy-information-panel position normalization.

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocumentAttribute, OrderedSourceDocumentChild, OrderedSourceDocumentChildren,
    OrderedSourceDocumentNode,
};

pub(super) fn normalize_authored_buy_information_panel_position(
    _attributes: &mut Vec<OrderedSourceDocumentAttribute>,
    children: &mut OrderedSourceDocumentChildren,
) {
    let dynamic_start_positions = children
        .items()
        .iter()
        .filter_map(|child| match child {
            OrderedSourceDocumentChild::Element(node) if node.name == "DynamicStartPos" => {
                Some(node)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let Some(dynamic_start_position) =
        (dynamic_start_positions.len() == 1).then(|| dynamic_start_positions[0])
    else {
        return;
    };
    let Some(initial_x) = dynamic_start_position.attribute("x") else {
        return;
    };
    let Some(initial_y) = dynamic_start_position.attribute("y") else {
        return;
    };

    // `DynamicStartPos` is not an inset for every child of ZTBuyInfoPanel.
    // The native buy-info refresh moves the dynamic cost/upkeep group to this
    // point while leaving the title and fixed preview controls at their
    // authored regions. Applying it as grid padding cannot move absolutely
    // positioned children and leaves the cost text on top of the title.
    let mut normalized_children = children
        .items()
        .iter()
        .filter(|child| {
            !matches!(child, OrderedSourceDocumentChild::Element(node) if node.name == "DynamicStartPos")
        })
        .cloned()
        .collect::<Vec<_>>();
    normalized_children.iter_mut().for_each(|child| {
        if let OrderedSourceDocumentChild::Element(node) = child {
            apply_authored_buy_information_dynamic_start_position_to_cost_layout(
                node, initial_x, initial_y,
            );
        }
    });
    *children = OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
        normalized_children,
    );
}

fn apply_authored_buy_information_dynamic_start_position_to_cost_layout(
    node: &mut OrderedSourceDocumentNode,
    initial_x: &str,
    initial_y: &str,
) {
    if node.name == "UILayout" && node.attribute("name") == Some("cost") {
        let mut children = node.children.items().to_vec();
        children.iter_mut().for_each(|child| {
            let OrderedSourceDocumentChild::Element(region) = child else {
                return;
            };
            if region.name != "UIRegion" {
                return;
            }
            for (attribute_name, value) in [("x", initial_x), ("y", initial_y)] {
                if let Some(attribute) = region
                    .attributes
                    .iter_mut()
                    .find(|attribute| attribute.name() == attribute_name)
                {
                    attribute.set_attribute_value(value);
                }
            }
        });
        node.children =
            OrderedSourceDocumentChildren::from_ordered_source_document_child_items(children);
        return;
    }

    let mut children = node.children.items().to_vec();
    children.iter_mut().for_each(|child| {
        if let OrderedSourceDocumentChild::Element(child) = child {
            apply_authored_buy_information_dynamic_start_position_to_cost_layout(
                child, initial_x, initial_y,
            );
        }
    });
    node.children =
        OrderedSourceDocumentChildren::from_ordered_source_document_child_items(children);
}
