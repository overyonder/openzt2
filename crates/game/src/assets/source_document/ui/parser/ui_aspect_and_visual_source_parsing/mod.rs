//! Authored UI aspect, visual-state, rectangle, color, and font parsing.

use crate::assets::source_document::{
    ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
};

use super::{
    super::model::{
        SourceUiAspect, SourceUiColor, SourceUiFont, SourceUiHitPolicy, SourceUiMetric,
        SourceUiMetricRect, SourceUiNamedVisual, SourceUiPayloadNode, SourceUiRect, SourceUiVisual,
    },
    source_scalar_and_attribute_reading::{
        copy_unknown_source_ui_attributes, parse_source_bool, parse_source_f32, parse_source_i32,
    },
};

pub(super) fn parse_authored_ui_aspect(node: &OrderedSourceDocumentNode) -> SourceUiAspect {
    let elements: Vec<_> = node.element_children().collect();
    let mut default = elements
        .iter()
        .copied()
        .find(|node| node.name == "default")
        .map(parse_authored_ui_visual);
    if let Some(color) = elements
        .iter()
        .copied()
        .find(|node| node.name == "BFColor")
        .and_then(parse_source_ui_color)
    {
        default
            .get_or_insert_with(|| SourceUiVisual {
                image: None,
                sound: None,
                hit_policy: None,
                rect: None,
                color: None,
                font: None,
                text_alignment: None,
                text_format: None,
                unknown_attributes: Vec::new(),
                unknown_elements: Vec::new(),
            })
            .color = Some(color);
    }
    if let Some(rect) = elements
        .iter()
        .copied()
        .find(|node| node.name == "BFRect")
        .map(parse_source_ui_metric_rectangle)
    {
        default
            .get_or_insert_with(|| SourceUiVisual {
                image: None,
                sound: None,
                hit_policy: None,
                rect: None,
                color: None,
                font: None,
                text_alignment: None,
                text_format: None,
                unknown_attributes: Vec::new(),
                unknown_elements: Vec::new(),
            })
            .rect = Some(rect);
    }
    let mut standard = elements
        .iter()
        .copied()
        .filter(|node| is_authored_visual_state(&node.name))
        .map(parse_named_authored_ui_visual)
        .collect::<Vec<_>>();
    standard.extend(
        elements
            .iter()
            .copied()
            .find(|node| node.name == "standard")
            .into_iter()
            .flat_map(|node| node.element_children())
            .map(parse_named_authored_ui_visual),
    );
    let alternate = elements
        .iter()
        .copied()
        .find(|node| node.name == "alternate")
        .into_iter()
        .flat_map(|node| node.element_children())
        .map(parse_named_authored_ui_visual)
        .collect();
    SourceUiAspect {
        localization_id: node
            .attribute("locid")
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        authored_string: node
            .attribute("string")
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        hit_policy: node
            .attribute("alwayshit")
            .map(SourceUiHitPolicy::parse)
            .unwrap_or(SourceUiHitPolicy::Normal),
        pad_x: node.attribute("padx").and_then(parse_source_i32),
        pad_y: node.attribute("pady").and_then(parse_source_i32),
        draw_3d: node
            .attribute("draw3D")
            .and_then(parse_source_bool)
            .unwrap_or(false),
        auto_size: node.attribute("autosize").and_then(parse_source_bool),
        default,
        standard,
        alternate,
        unknown_attributes: copy_unknown_source_ui_attributes(
            &node.attributes,
            &[
                "locid",
                "string",
                "alwayshit",
                "padx",
                "pady",
                "draw3D",
                "autosize",
            ],
        ),
        unknown_elements: elements
            .into_iter()
            .filter(|node| {
                !matches!(
                    node.name.as_str(),
                    "default"
                        | "standard"
                        | "alternate"
                        | "normal"
                        | "highlighted"
                        | "activated"
                        | "disabled"
                        | "BFRect"
                        | "BFColor"
                        | "text"
                )
            })
            .map(SourceUiPayloadNode::from_data_node)
            .collect(),
    }
}

pub(super) fn create_text_only_ui_aspect(text: &str) -> SourceUiAspect {
    SourceUiAspect {
        localization_id: None,
        authored_string: Some(text.to_owned()),
        hit_policy: SourceUiHitPolicy::Normal,
        pad_x: None,
        pad_y: None,
        draw_3d: false,
        auto_size: None,
        default: None,
        standard: Vec::new(),
        alternate: Vec::new(),
        unknown_attributes: Vec::new(),
        unknown_elements: Vec::new(),
    }
}

pub(super) fn parse_direct_default_ui_aspect(
    default: &OrderedSourceDocumentNode,
) -> SourceUiAspect {
    SourceUiAspect {
        localization_id: None,
        authored_string: None,
        hit_policy: SourceUiHitPolicy::Normal,
        pad_x: None,
        pad_y: None,
        draw_3d: false,
        auto_size: None,
        default: Some(parse_authored_ui_visual(default)),
        standard: Vec::new(),
        alternate: Vec::new(),
        unknown_attributes: Vec::new(),
        unknown_elements: Vec::new(),
    }
}

fn parse_named_authored_ui_visual(node: &OrderedSourceDocumentNode) -> SourceUiNamedVisual {
    SourceUiNamedVisual {
        name: node.name.to_string(),
        visual: parse_authored_ui_visual(node),
    }
}

fn is_authored_visual_state(name: &str) -> bool {
    matches!(name, "normal" | "highlighted" | "activated" | "disabled")
}

fn parse_authored_ui_visual(node: &OrderedSourceDocumentNode) -> SourceUiVisual {
    let elements: Vec<_> = node.element_children().collect();
    SourceUiVisual {
        image: node
            .attribute("image")
            .or_else(|| node.attribute("sssimage"))
            .filter(|value| !value.is_empty())
            .map(AssetPath::new),
        sound: node
            .attribute("sound")
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        hit_policy: node.attribute("alwayshit").map(SourceUiHitPolicy::parse),
        rect: elements
            .iter()
            .copied()
            .find(|node| node.name == "BFRect")
            .map(parse_source_ui_metric_rectangle)
            .or_else(|| {
                node.attributes
                    .iter()
                    .any(|attribute| matches!(attribute.name(), "x" | "y" | "w" | "h"))
                    .then(|| parse_source_ui_metric_rectangle(node))
            }),
        color: elements
            .iter()
            .copied()
            .find(|node| matches!(node.name.as_str(), "BFColor" | "UIColorObj"))
            .and_then(parse_source_ui_color),
        font: elements
            .iter()
            .copied()
            .find(|node| node.name == "BFFont")
            .map(parse_authored_ui_font),
        text_alignment: node.attribute("align").map(str::to_owned),
        text_format: node.attribute("format").map(str::to_owned),
        unknown_attributes: copy_unknown_source_ui_attributes(
            &node.attributes,
            &[
                "image",
                "sssimage",
                "sound",
                "alwayshit",
                "x",
                "y",
                "w",
                "h",
                "align",
                "format",
            ],
        ),
        unknown_elements: elements
            .into_iter()
            .filter(|node| {
                !matches!(
                    node.name.as_str(),
                    "BFRect" | "BFColor" | "UIColorObj" | "BFFont"
                )
            })
            .map(SourceUiPayloadNode::from_data_node)
            .collect(),
    }
}

pub(super) fn parse_source_ui_metric_rectangle(
    node: &OrderedSourceDocumentNode,
) -> SourceUiMetricRect {
    SourceUiMetricRect {
        x: SourceUiMetric::parse(node.attribute("x")),
        y: SourceUiMetric::parse(node.attribute("y")),
        width: SourceUiMetric::parse(node.attribute("w")),
        height: SourceUiMetric::parse(node.attribute("h")),
    }
}

pub(super) fn parse_source_ui_rectangle(node: &OrderedSourceDocumentNode) -> Option<SourceUiRect> {
    Some(SourceUiRect {
        x: node.attribute("x")?.parse().ok()?,
        y: node.attribute("y")?.parse().ok()?,
        width: node.attribute("w")?.parse().ok()?,
        height: node.attribute("h")?.parse().ok()?,
    })
}

pub(super) fn parse_source_ui_color(node: &OrderedSourceDocumentNode) -> Option<SourceUiColor> {
    Some(SourceUiColor {
        red: node.attribute("r")?.parse().ok()?,
        green: node.attribute("g")?.parse().ok()?,
        blue: node.attribute("b")?.parse().ok()?,
        alpha: node
            .attribute("a")
            .and_then(|value| value.parse().ok())
            .unwrap_or(255),
    })
}

fn parse_authored_ui_font(node: &OrderedSourceDocumentNode) -> SourceUiFont {
    SourceUiFont {
        face: node
            .attribute("font")
            .or_else(|| node.attribute("name"))
            .map(str::to_owned),
        size: node.attribute("size").and_then(parse_source_f32),
        align: node
            .attribute("align")
            .or_else(|| node.attribute("aligh"))
            .map(str::to_owned),
        x: node.attribute("x").and_then(parse_source_i32),
        y: node.attribute("y").and_then(parse_source_i32),
        shadow_x: node.attribute("shadowx").and_then(parse_source_i32),
        shadow_y: node.attribute("shadowy").and_then(parse_source_i32),
        bold: node.attribute("bold").and_then(parse_source_bool),
        underline: node.attribute("underline").and_then(parse_source_bool),
        color: node
            .element_children()
            .find(|node| node.name == "BFColor")
            .and_then(parse_source_ui_color),
        image: node
            .attribute("image")
            .filter(|value| !value.is_empty())
            .map(AssetPath::new),
        unknown_attributes: copy_unknown_source_ui_attributes(
            &node.attributes,
            &[
                "font",
                "name",
                "size",
                "align",
                "aligh",
                "x",
                "y",
                "w",
                "shadowx",
                "shadowy",
                "bold",
                "underline",
                "image",
            ],
        ),
    }
}
