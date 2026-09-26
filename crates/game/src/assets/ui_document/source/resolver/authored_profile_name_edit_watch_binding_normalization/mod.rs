//! Authored profile-name edit-watch lowering into a typed property binding.

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentAttribute, OrderedSourceDocumentChild,
    OrderedSourceDocumentChildren, OrderedSourceDocumentNode,
};

use super::super::ui_source_document_gap::{
    UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
};

pub(super) fn normalize_authored_profile_name_edit_watch_bindings(
    document: &mut OrderedSourceDocument,
) -> Result<(), UiSourceDocumentGap> {
    let mut edit_watch_target_names = Vec::new();
    collect_authored_profile_name_edit_watch_target_names(
        &mut document.root.children,
        &mut edit_watch_target_names,
    );
    for target_name in edit_watch_target_names {
        if !attach_profile_present_binding_to_named_authored_ui_node(
            &mut document.root.children,
            &target_name,
        ) {
            return Err(UiSourceDocumentGap::at_root(
                document,
                UiSourceDocumentFamily::Ui,
                UiSourceDocumentGapKind::UnsupportedVocabulary,
                format!("UI_EDIT_WATCH target {target_name:?} does not name an authored UI node"),
            ));
        }
    }
    Ok(())
}

/// `UITextEdit <edit>` is the original change-observer block. The shipped
/// `UI_EDIT_WATCH` form enables its target while the profile-name edit has a
/// value. Resolve that cross-widget observer during source normalization to
/// the typed `Enabled(ProfilePresent)` binding.
///
/// An unfamiliar `<edit>` payload remains in the ordered source tree so the
/// ordinary source diagnostic rejects it instead of discarding behavior.
fn collect_authored_profile_name_edit_watch_target_names(
    children: &mut OrderedSourceDocumentChildren,
    target_names: &mut Vec<String>,
) {
    let mut retained_children = Vec::with_capacity(children.items().len());
    for child in children.items().iter().cloned() {
        match child {
            OrderedSourceDocumentChild::Element(mut node) => {
                collect_authored_profile_name_edit_watch_target_names(
                    &mut node.children,
                    target_names,
                );
                if node.name == "UITextEdit" {
                    let source_name = node.attribute("name").unwrap_or_default().to_owned();
                    let mut retained_node_children =
                        Vec::with_capacity(node.children.items().len());
                    for child in node.children.items().iter().cloned() {
                        match child {
                            OrderedSourceDocumentChild::Element(edit) if edit.name == "edit" => {
                                if let Some(mut watched_target_names) =
                                    authored_profile_name_edit_watch_target_names(
                                        &edit,
                                        &source_name,
                                    )
                                {
                                    target_names.append(&mut watched_target_names);
                                } else {
                                    retained_node_children
                                        .push(OrderedSourceDocumentChild::Element(edit));
                                }
                            }
                            child => retained_node_children.push(child),
                        }
                    }
                    node.children =
                        OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
                            retained_node_children,
                        );
                }
                retained_children.push(OrderedSourceDocumentChild::Element(node));
            }
            child => retained_children.push(child),
        }
    }
    *children =
        OrderedSourceDocumentChildren::from_ordered_source_document_child_items(retained_children);
}

fn authored_profile_name_edit_watch_target_names(
    edit: &OrderedSourceDocumentNode,
    source_name: &str,
) -> Option<Vec<String>> {
    let events = edit.element_children().collect::<Vec<_>>();
    if events.is_empty() {
        return None;
    }
    events
        .into_iter()
        .map(|event| {
            if event.name != "event" || event.attribute("msg") != Some("UI_CHILD") {
                return None;
            }
            let target_name = event.attribute("name")?.to_owned();
            let watch = event.element_children().find(|node| {
                matches!(node.name.as_str(), "child" | "event")
                    && node.attribute("msg") == Some("UI_EDIT_WATCH")
            })?;
            (watch.attribute("string") == Some(source_name)).then_some(target_name)
        })
        .collect()
}

fn attach_profile_present_binding_to_named_authored_ui_node(
    children: &mut OrderedSourceDocumentChildren,
    target_name: &str,
) -> bool {
    let mut attached = false;
    let normalized_children = children
        .items()
        .iter()
        .cloned()
        .map(|child| match child {
            OrderedSourceDocumentChild::Element(mut node) => {
                if node.attribute("name") == Some(target_name) {
                    let already_present = node.element_children().any(|child| {
                        child.name == "field"
                            && child.attribute("name") == Some("enabled")
                            && child.attribute("type") == Some("bool")
                            && child.attribute("fmt") == Some("openzt2:profile-present")
                    });
                    if !already_present {
                        let mut node_children = node.children.items().to_vec();
                        node_children.push(OrderedSourceDocumentChild::Element(Box::new(
                            OrderedSourceDocumentNode::new_synthetic_ordered_source_document_node(
                                "field".into(),
                                vec![
                                    OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute("name", "enabled"),
                                    OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute("type", "bool"),
                                    OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute("fmt", "openzt2:profile-present"),
                                ],
                                OrderedSourceDocumentChildren::default(),
                                node.span,
                            ),
                        )));
                        node.children = OrderedSourceDocumentChildren::from_ordered_source_document_child_items(node_children);
                    }
                    attached = true;
                } else if attach_profile_present_binding_to_named_authored_ui_node(
                    &mut node.children,
                    target_name,
                ) {
                    attached = true;
                }
                OrderedSourceDocumentChild::Element(node)
            }
            child => child,
        })
        .collect();
    *children = OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
        normalized_children,
    );
    attached
}
