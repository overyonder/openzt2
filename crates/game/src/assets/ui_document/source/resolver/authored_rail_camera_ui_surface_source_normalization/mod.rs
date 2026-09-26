//! Authored rail-camera UI surface normalization.

use crate::assets::source_document::ordered_source_document_types::{
    OrderedSourceDocumentAttribute, OrderedSourceDocumentChild, OrderedSourceDocumentChildren,
};

pub(super) fn attach_rail_camera_scene_path_to_authored_ui_source_tree(
    children: &mut OrderedSourceDocumentChildren,
    rail_camera_scene_path: &str,
) {
    let normalized_children = children
        .items()
        .iter()
        .cloned()
        .map(|child| match child {
            OrderedSourceDocumentChild::Element(mut node) => {
                if node.name == "ZTRailCam"
                    && node
                        .attributes
                        .iter()
                        .all(|attribute| attribute.name() != "openzt2RailCameraScene")
                {
                    node.attributes.push(
                        OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
                            "openzt2RailCameraScene",
                            rail_camera_scene_path,
                        ),
                    );
                }
                attach_rail_camera_scene_path_to_authored_ui_source_tree(
                    &mut node.children,
                    rail_camera_scene_path,
                );
                OrderedSourceDocumentChild::Element(node)
            }
            child => child,
        })
        .collect();
    *children = OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
        normalized_children,
    );
}

/// The original rail-camera control combines a rectangular UI compositing
/// surface with a separately lowered scene and camera document. Retain only
/// the ordinary region, state, and aspect surface in the UI source tree.
pub(super) fn normalize_authored_rail_camera_ui_surface(
    attributes: &mut Vec<OrderedSourceDocumentAttribute>,
    children: &mut OrderedSourceDocumentChildren,
) {
    attributes.retain(|attribute| {
        matches!(
            attribute.name(),
            "name" | "template" | "templateName" | "openzt2RailCameraScene"
        )
    });
    let retained_surface_children = children
        .items()
        .iter()
        .filter_map(|child| match child {
            OrderedSourceDocumentChild::Element(node)
                if matches!(node.name.as_str(), "UIRegion" | "UIState" | "UIAspect") =>
            {
                let mut node = node.clone();
                if node.name == "UIAspect" {
                    node.attributes
                        .retain(|attribute| attribute.name() != "draw3D");
                }
                Some(OrderedSourceDocumentChild::Element(node))
            }
            OrderedSourceDocumentChild::Text(text) => {
                Some(OrderedSourceDocumentChild::Text(text.clone()))
            }
            _ => None,
        })
        .collect();
    *children = OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
        retained_surface_children,
    );
}
