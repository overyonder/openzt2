//! Authored globe model, transform, and interaction-property extraction.

use crate::assets::source_document::{
    ordered_source_document_types::{OrderedSourceDocumentAttribute, OrderedSourceDocumentNode},
    path::AssetPath,
};

use super::{
    super::model::SourceUiGlobe,
    source_scalar_and_attribute_reading::{
        parse_source_f32, parse_source_f32_attribute, source_attribute_value,
    },
    ui_aspect_and_visual_source_parsing::parse_source_ui_color as parse_color,
};

pub(super) fn extract_authored_ui_globe_source(
    attributes: &[OrderedSourceDocumentAttribute],
    children: &[&OrderedSourceDocumentNode],
) -> SourceUiGlobe {
    let child = |name: &str| children.iter().copied().find(|node| node.name == name);
    let model = |name: &str| child(name).and_then(descendant_model);
    let translation = |name: &str| child(name).and_then(descendant_physical_translation);
    let secondary_models = children
        .iter()
        .copied()
        .find(|node| node.name == "secondaryGlobe")
        .into_iter()
        .flat_map(|node| node.element_children())
        .filter_map(|node| descendant_model(node).map(|path| (node.name.to_string(), path)))
        .collect();
    SourceUiGlobe {
        primary_model: model("globe"),
        primary_translation: translation("globe"),
        clouds_model: model("clouds"),
        clouds_translation: translation("clouds"),
        dot_model: model("dot"),
        dot_translation: translation("dot"),
        selected_dot_model: model("selected").or_else(|| model("dotselected")),
        selected_dot_translation: translation("selected").or_else(|| translation("dotselected")),
        pointer_model: model("highlighted").or_else(|| model("pointer")),
        dot_highlight_cursor: source_attribute_value(attributes, "dotHighlightCursor")
            .map(str::to_owned),
        secondary_models,
        selection_rotate_speed: parse_source_f32_attribute(attributes, "selectionRotateSpeed"),
        mouse_increment: parse_source_f32_attribute(attributes, "mouseIncrement"),
        mouse_down_friction: parse_source_f32_attribute(attributes, "mouseDownFriction"),
        mouse_up_friction: parse_source_f32_attribute(attributes, "mouseUpFriction"),
        friction_transition_seconds: parse_source_f32_attribute(
            attributes,
            "frictionTransitionTime",
        ),
        move_seconds: parse_source_f32_attribute(attributes, "moveTime"),
        scream_threshold: parse_source_f32_attribute(attributes, "screamThreshold"),
        scream_delay_seconds: parse_source_f32_attribute(attributes, "screamDelay"),
        dot_highlight_color: children
            .iter()
            .copied()
            .find(|node| node.name == "dotHighlightColor")
            .and_then(parse_color),
    }
}

fn descendant_physical_translation(node: &OrderedSourceDocumentNode) -> Option<[f32; 3]> {
    (node.name == "BFPhysObj")
        .then(|| {
            node.element_children()
                .find(|child| child.name == "position")
        })
        .flatten()
        .and_then(|position| {
            Some([
                position.attribute("x").and_then(parse_source_f32)?,
                position.attribute("y").and_then(parse_source_f32)?,
                position.attribute("z").and_then(parse_source_f32)?,
            ])
        })
        .or_else(|| {
            node.element_children()
                .find_map(descendant_physical_translation)
        })
}

fn descendant_model(node: &OrderedSourceDocumentNode) -> Option<AssetPath> {
    (node.name == "BFSceneGraphComponent")
        .then(|| node.attribute("modelfile"))
        .flatten()
        .filter(|value| !value.is_empty())
        .map(AssetPath::new)
        .or_else(|| node.element_children().find_map(descendant_model))
}
