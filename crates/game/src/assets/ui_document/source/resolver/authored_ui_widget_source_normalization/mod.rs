//! Observed authored-widget repairs before canonical UI source parsing.

use crate::assets::original_game_asset_typo_fixups::is_misspelled_children_wrapper;
use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocument, OrderedSourceDocumentChild, OrderedSourceDocumentChildren,
    OrderedSourceDocumentNode,
};

use super::{
    super::ui_source_document_gap::UiSourceDocumentGap,
    authored_buy_information_panel_source_normalization::normalize_authored_buy_information_panel_position,
    authored_money_text_source_normalization::normalize_authored_money_text_widget,
    authored_profile_name_edit_watch_binding_normalization::normalize_authored_profile_name_edit_watch_bindings,
    authored_rail_camera_ui_surface_source_normalization::{
        attach_rail_camera_scene_path_to_authored_ui_source_tree,
        normalize_authored_rail_camera_ui_surface,
    },
    authored_scenario_objective_collection_source_normalization::normalize_authored_scenario_objective_collection,
    authored_zoopedia_table_of_contents_source_normalization::normalize_authored_zoopedia_table_of_contents_collection,
};

pub(super) fn normalize_observed_authored_ui_widget_source_shapes(
    document: &OrderedSourceDocument,
) -> Result<OrderedSourceDocument, UiSourceDocumentGap> {
    let mut document = document.clone();
    let path = document.path.key();
    let stem = path
        .rsplit('/')
        .next()
        .and_then(|name| {
            name.rsplit_once('.')
                .map_or(Some(name), |(stem, _)| Some(stem))
        })
        .unwrap_or_default();
    if document.root.attribute("template") == Some(stem)
        && document.root.attribute("templateName").is_none()
    {
        // Standalone row-template documents use their file stem in the
        // `template` slot as an external catalogue name. They are complete
        // definitions, not recursive inheritance from a missing widget.
        document
            .root
            .attributes
            .retain(|attribute| attribute.name() != "template");
    }
    normalize_authored_profile_name_edit_watch_bindings(&mut document)?;
    let rail_scene = format!("{path}#RailCamera");
    normalize_root(&mut document.root, &rail_scene);
    Ok(document)
}

fn normalize_root(root: &mut OrderedSourceDocumentNode, rail_scene: &str) {
    attach_rail_camera_scene_path_to_authored_ui_source_tree(&mut root.children, rail_scene);
    root.children = normalize_children(&root.children);
    if root.name == "UITool" {
        root.children = OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
            root.children
                .items()
                .iter()
                .filter(|child| {
                    !matches!(child, OrderedSourceDocumentChild::Element(child) if child.name == "templates")
                })
                .cloned()
                .collect(),
        );
    }
    match root.name.as_str() {
        "ZTRailCam" => {
            normalize_authored_rail_camera_ui_surface(&mut root.attributes, &mut root.children);
        }
        "ZTMoneyText" => {
            normalize_authored_money_text_widget(
                &mut root.attributes,
                &mut root.children,
                root.span,
            );
            root.name = "UIText".into();
        }
        "ZTBuyInfoPanel" => {
            normalize_authored_buy_information_panel_position(
                &mut root.attributes,
                &mut root.children,
            );
            root.name = "UILayout".into();
        }
        _ => {}
    }
}

fn normalize_node(node: &mut OrderedSourceDocumentNode) {
    if is_misspelled_children_wrapper(node) {
        node.name = "children".into();
    }
    // ZTZoopediaTOC's authored `entry` wrapper is the row-template boundary.
    // Extract it before ordinary child normalization turns generic `entry`
    // wrappers into `children` containers.
    normalize_authored_zoopedia_table_of_contents_collection(node);
    if node.name == "ZTRailCam" {
        normalize_authored_rail_camera_ui_surface(&mut node.attributes, &mut node.children);
        node.children = normalize_children(&node.children);
        return;
    }
    node.children = normalize_children(&node.children);
    // Two shipped confirmation layouts place an ordinary child UILayout
    // inside UIAspect. The original loader hoisted it into the widget tree;
    // perform that source repair once rather than retaining an aspect payload
    // that Bevy UI could not own.
    let mut hoisted = Vec::new();
    let mut repaired = node
        .children
        .items()
        .iter()
        .cloned()
        .map(|child| match child {
            OrderedSourceDocumentChild::Element(mut aspect) if aspect.name == "UIAspect" => {
                let kept = aspect
                    .children
                    .items()
                    .iter()
                    .filter_map(|child| match child {
                        OrderedSourceDocumentChild::Element(layout)
                            if layout.name == "UILayout" =>
                        {
                            hoisted.push(OrderedSourceDocumentChild::Element(layout.clone()));
                            None
                        }
                        child => Some(child.clone()),
                    })
                    .collect();
                aspect.children =
                    OrderedSourceDocumentChildren::from_ordered_source_document_child_items(kept);
                OrderedSourceDocumentChild::Element(aspect)
            }
            child => child,
        })
        .collect::<Vec<_>>();
    repaired.extend(hoisted);
    node.children =
        OrderedSourceDocumentChildren::from_ordered_source_document_child_items(repaired);
    normalize_authored_scenario_objective_collection(node);
    match node.name.as_str() {
        // Zoopedia and status systems populate these layout children.
        "ZTZoopediaComponent" | "ZTZooStatusComponent" => {
            node.name = "UILayout".into();
        }
        // Credits use a layout with a separate playback timeline.
        "ZTCredits" => {
            node.name = "UILayout".into();
        }
        _ => {}
    }
    let normalized_children = node
        .children
        .items()
        .iter()
        .filter_map(|child| match child {
            // BFRect is the old renderer's source-rectangle carrier. Its
            // useful occurrences are already consumed inside UI visuals.
            OrderedSourceDocumentChild::Element(child)
                if node.name == "UIText" && child.name == "BFRect" =>
            {
                None
            }
            // Ordinary entry wrappers carry authored child widgets rather
            // than representing a typed widget themselves.
            OrderedSourceDocumentChild::Element(child)
                if child.name == "entry" && node.name != "images" =>
            {
                let mut child = child.as_ref().clone();
                child.name = "children".into();
                Some(OrderedSourceDocumentChild::Element(Box::new(child)))
            }
            OrderedSourceDocumentChild::Element(child)
                if node.name == "UITextEdit" && child.name == "edit" =>
            {
                let mut child = child.as_ref().clone();
                child.name = "textchanged".into();
                Some(OrderedSourceDocumentChild::Element(Box::new(child)))
            }
            OrderedSourceDocumentChild::Element(child)
                if node.name == "UISlider" && child.name == "onChange" =>
            {
                let mut child = child.as_ref().clone();
                child.name = "textchanged".into();
                Some(OrderedSourceDocumentChild::Element(Box::new(child)))
            }
            OrderedSourceDocumentChild::Element(child)
                if node.name == "UIDrag" && child.name == "dragevents" =>
            {
                let mut child = child.as_ref().clone();
                child.name = "textchanged".into();
                Some(OrderedSourceDocumentChild::Element(Box::new(child)))
            }
            OrderedSourceDocumentChild::Element(child)
                if node.name == "ZTUIFullscreenButton" && child.name == "rightmousedown" =>
            {
                let mut child = child.as_ref().clone();
                child.name = "activate".into();
                Some(OrderedSourceDocumentChild::Element(Box::new(child)))
            }
            child => Some(child.clone()),
        })
        .collect::<Vec<_>>();
    node.children = OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
        normalized_children,
    );
    if node.name == "UITool" {
        // The shipped dynamic information panel embeds an old ZTAI thinker
        // template under the visual tool node. Information projection is now
        // driven by typed selected-entity bindings, so the legacy thinker
        // graph is source composition metadata rather than a native UI child.
        node.children = OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
            node.children
                .items()
                .iter()
                .filter(|child| {
                    !matches!(child, OrderedSourceDocumentChild::Element(child) if child.name == "templates")
                })
                .cloned()
                .collect(),
        );
    }
    match node.name.as_str() {
        "ZTMoneyText" => {
            normalize_authored_money_text_widget(
                &mut node.attributes,
                &mut node.children,
                node.span,
            );
            node.name = "UIText".into();
        }
        "ZTBuyInfoPanel" => {
            normalize_authored_buy_information_panel_position(
                &mut node.attributes,
                &mut node.children,
            );
            node.name = "UILayout".into();
        }
        _ => {}
    }
}

fn normalize_children(children: &OrderedSourceDocumentChildren) -> OrderedSourceDocumentChildren {
    OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
        children
            .items()
            .iter()
            .filter(
                |child| !matches!(child, OrderedSourceDocumentChild::Element(node) if is_empty_extension_slot(node)),
            )
            .cloned()
            .map(|child| match child {
                OrderedSourceDocumentChild::Element(mut node) => {
                    normalize_node(&mut node);
                    OrderedSourceDocumentChild::Element(node)
                }
                child => child,
            })
            .collect(),
    )
}

/// Blue Fang layouts may carry an empty `<extra/>` extension slot after their
/// authored children. It has no attributes, payload, or executable events and
/// therefore contributes no native UI fact. Only that provably empty form is
/// omitted; a populated extension remains visible to the typed parser and is
/// rejected until its semantics are supported.
fn is_empty_extension_slot(node: &OrderedSourceDocumentNode) -> bool {
    node.name == "extra"
        && node.attributes.is_empty()
        && node.children.items().iter().all(|child| match child {
            OrderedSourceDocumentChild::Text(text) => text.value().trim().is_empty(),
            OrderedSourceDocumentChild::Comment => true,
            OrderedSourceDocumentChild::Element(_)
            | OrderedSourceDocumentChild::ProcessingInstruction => false,
        })
}
