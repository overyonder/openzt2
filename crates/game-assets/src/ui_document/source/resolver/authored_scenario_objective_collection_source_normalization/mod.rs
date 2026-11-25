//! Authored scenario-objective collection and row-template normalization.

use crate::source_document::{
    ordered_source_document_types::{
        OrderedSourceDocumentAttribute, OrderedSourceDocumentChild, OrderedSourceDocumentChildren,
        OrderedSourceDocumentNode,
    },
    ui::model::SourceUiWidgetKind,
};

pub(in crate::ui_document::source) const OBJECTIVE_GROUP_TEMPLATE_FIELD: &str =
    "openzt2ObjectiveGroupTemplate";
pub(super) const OBJECTIVE_GROUP_TEMPLATE_PREFIX: &str = "openzt2_scenario_objective_group_";
pub(super) const OBJECTIVE_ROW_TEMPLATE_PREFIX: &str = "openzt2_scenario_objective_row_";

pub(super) fn normalize_authored_scenario_objective_collection(
    node: &mut OrderedSourceDocumentNode,
    document: openzt2_game_data::AssetId,
) {
    if node.name != "ZTGoalPanel" {
        return;
    }

    let template_owner = format!("{:032x}_{}", u128::from_le_bytes(document.0), node.span.start);
    let row_template_name = format!("{OBJECTIVE_ROW_TEMPLATE_PREFIX}{template_owner}");
    let objective_row_template =
        create_authored_scenario_objective_row_template(node, &row_template_name);
    let scenario_overview = create_authored_scenario_overview_layout(node);
    let group_template = node
        .element_children()
        .find(|child| child.name == "group")
        .and_then(|group| {
            group
                .element_children()
                .find(|widget| SourceUiWidgetKind::is_known_tag(&widget.name))
        })
        .cloned();

    node.name = "UIListBox".into();
    node.attributes.push(
        OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
            "openzt2ListSource",
            "scenarioObjectives",
        ),
    );

    let mut normalized_children = node
        .children
        .items()
        .iter()
        .filter(|child| {
            !matches!(
                child,
                OrderedSourceDocumentChild::Element(child)
                    if matches!(
                        child.name.as_str(),
                        "overview" | "group" | "neutral" | "success" | "failure"
                    )
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    if let Some(objective_row_template) = objective_row_template {
        node.attributes.push(
            OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
                "openzt2RowTemplate",
                row_template_name,
            ),
        );
        normalized_children.push(OrderedSourceDocumentChild::Element(Box::new(
            objective_row_template,
        )));
    }
    if let Some(scenario_overview) = scenario_overview {
        normalized_children.push(OrderedSourceDocumentChild::Element(Box::new(
            scenario_overview,
        )));
    }
    if let Some(mut group_template) = group_template {
        let template_name = format!("{OBJECTIVE_GROUP_TEMPLATE_PREFIX}{template_owner}");
        group_template
            .attributes
            .retain(|attribute| attribute.name() != "templateName");
        group_template.attributes.push(
            OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
                "templateName",
                template_name.as_str(),
            ),
        );
        let field = OrderedSourceDocumentNode::new_synthetic_ordered_source_document_node(
            "field".into(),
            vec![
                OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
                    "name",
                    OBJECTIVE_GROUP_TEMPLATE_FIELD,
                ),
                OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
                    "fmt",
                    template_name,
                ),
            ],
            OrderedSourceDocumentChildren::from_ordered_source_document_child_items(Vec::new()),
            node.span,
        );
        normalized_children.extend(
            [group_template, field]
                .map(|child| OrderedSourceDocumentChild::Element(Box::new(child))),
        );
    }
    node.children = OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
        normalized_children,
    );
}

fn create_authored_scenario_objective_row_template(
    node: &OrderedSourceDocumentNode,
    template_name: &str,
) -> Option<OrderedSourceDocumentNode> {
    let objective_visual_rows = node
        .children
        .items()
        .iter()
        .filter_map(|child| match child {
            OrderedSourceDocumentChild::Element(row)
                if matches!(row.name.as_str(), "neutral" | "success" | "failure") =>
            {
                let mut row = row.as_ref().clone();
                let mut row_children = row.children.items().to_vec();
                if let Some(OrderedSourceDocumentChild::Element(widget)) =
                    row_children.iter_mut().find(|child| {
                        matches!(
                            child,
                            OrderedSourceDocumentChild::Element(widget)
                                if SourceUiWidgetKind::is_known_tag(&widget.name)
                        )
                    })
                {
                    widget.attributes.push(
                        OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
                            "openzt2ScenarioObjectiveVisual",
                            row.name.clone(),
                        ),
                    );
                }
                row.children =
                    OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
                        row_children,
                    );
                Some(OrderedSourceDocumentChild::Element(Box::new(row)))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    (!objective_visual_rows.is_empty()).then(|| {
        OrderedSourceDocumentNode::new_synthetic_ordered_source_document_node(
            "UILayout".into(),
            vec![
                OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
                    "name",
                    "openzt2 scenario objective row",
                ),
                OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
                    "templateName",
                    template_name,
                ),
                OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
                    "autosize", "true",
                ),
                // Only the active status contributes to the objective row's
                // height; its authored list must participate in normal flow.
                OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
                    "rows", "-1",
                ),
            ],
            OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
                objective_visual_rows,
            ),
            node.span,
        )
    })
}

fn create_authored_scenario_overview_layout(
    node: &OrderedSourceDocumentNode,
) -> Option<OrderedSourceDocumentNode> {
    node.children.items().iter().find_map(|child| match child {
        OrderedSourceDocumentChild::Element(overview) if overview.name == "overview" => {
            // `overview` is a template envelope, not a layout widget. Its
            // authored list owns the remaining panel width and intrinsic
            // height. An extra zero-sized UILayout here collapses that list
            // and gives its inset text a negative stretch width in Taffy.
            let mut overview = overview
                .children
                .items()
                .iter()
                .find_map(|child| match child {
                    OrderedSourceDocumentChild::Element(widget)
                        if SourceUiWidgetKind::is_known_tag(&widget.name) =>
                    {
                        Some(widget.as_ref().clone())
                    }
                    _ => None,
                })?;
            if let Some(name) = overview
                .attributes
                .iter_mut()
                .find(|attribute| attribute.name() == "name")
            {
                name.set_attribute_value("openzt2 scenario overview");
            } else {
                overview.attributes.push(
                    OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
                        "name",
                        "openzt2 scenario overview",
                    ),
                );
            }
            rename_authored_scenario_overview_text(&mut overview);
            Some(overview)
        }
        _ => None,
    })
}

fn rename_authored_scenario_overview_text(node: &mut OrderedSourceDocumentNode) {
    if node.name == "UIText" {
        if let Some(name) = node
            .attributes
            .iter_mut()
            .find(|attribute| attribute.name() == "name" && attribute.value() == "label")
        {
            name.set_attribute_value("openzt2 scenario overview text");
        }
    }
    let mut children = node.children.items().to_vec();
    children.iter_mut().for_each(|child| {
        if let OrderedSourceDocumentChild::Element(child) = child {
            rename_authored_scenario_overview_text(child);
        }
    });
    node.children =
        OrderedSourceDocumentChildren::from_ordered_source_document_child_items(children);
}
