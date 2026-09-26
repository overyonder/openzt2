use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    node::{UiNodeDefinition, UiNodeFlags, UiNodeKind},
    node_layout::{UiNodeRegionAlignment, UiNodeRegionDefinition, UiNodeRegionMetric},
    node_style::{UiStyleFlags, UiStyleRecord},
};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

pub(super) fn authored_list_ancestor<'a>(
    document: &'a UiDocumentAsset,
    index: u32,
) -> Option<&'a UiNodeDefinition> {
    let canonical_ui_document = document.canonical_ui_document();
    let mut parent = canonical_ui_document
        .nodes
        .get(index as usize)
        .and_then(|node| (node.parent != u32::MAX).then_some(node.parent));
    while let Some(index) = parent {
        let ancestor = canonical_ui_document.nodes.get(index as usize)?;
        if matches!(&ancestor.kind, UiNodeKind::List) {
            return Some(ancestor);
        }
        parent = (ancestor.parent != u32::MAX).then_some(ancestor.parent);
    }
    None
}

pub(super) fn project_authored_node_and_style_to_bevy_layout(
    record: &UiNodeDefinition,
    style: &UiStyleRecord,
    flags: u16,
) -> Node {
    let padding = style.padding.map(|value| value);
    let font_offset = style.font_offset.map(|value| value as f32);
    let border = style.border.map(|value| value);
    let nine_slice = style.flags.0 & UiStyleFlags::NINE_SLICE.0 != 0;
    let mut node = Node {
        position_type: PositionType::Absolute,
        padding: UiRect::new(
            px(padding[0] + font_offset[0]),
            px(padding[2]),
            px(padding[1] + font_offset[1]),
            px(padding[3]),
        ),
        border: if nine_slice {
            UiRect::ZERO
        } else {
            UiRect::new(px(border[0]), px(border[2]), px(border[1]), px(border[3]))
        },
        overflow: if matches!(&record.kind, UiNodeKind::Scroll) {
            Overflow::scroll_y()
        } else if flags & UiNodeFlags::CLIP_CHILDREN.0 != 0 {
            Overflow::clip()
        } else {
            Overflow::visible()
        },
        ..default()
    };
    apply_authored_node_region_to_bevy_node(&mut node, &record.region);
    let background_width = style.background_width;
    if background_width > 0.0 && matches!(node.width, Val::Auto | Val::Px(0.0)) {
        node.width = px(background_width);
    }
    node
}

pub(super) fn apply_authored_node_region_to_bevy_node(
    node: &mut Node,
    region: &UiNodeRegionDefinition,
) {
    let metric = |value: &UiNodeRegionMetric| match value {
        UiNodeRegionMetric::Pixels(value) => *value,
        // OUT_TOP is evidenced only on tooltip top strips and resolves to the
        // owning top edge. Asset loading rejects the three unrecovered spellings.
        UiNodeRegionMetric::OutsideTop => 0.0,
    };
    apply_horizontal_region(
        node,
        metric(&region.metrics[0]),
        metric(&region.metrics[2]),
        &region.alignment[0],
        &region.alignment[2],
    );
    apply_vertical_region(
        node,
        metric(&region.metrics[1]),
        metric(&region.metrics[3]),
        &region.alignment[1],
        &region.alignment[3],
    );
}

fn apply_horizontal_region(
    node: &mut Node,
    x: f32,
    width: f32,
    x_alignment: &UiNodeRegionAlignment,
    width_alignment: &UiNodeRegionAlignment,
) {
    match (x_alignment, width_alignment) {
        (
            UiNodeRegionAlignment::PercentageFromMinimum,
            UiNodeRegionAlignment::PercentageFromMinimum,
        ) => {
            node.left = percent(x);
            node.right = percent((100.0 - width).max(0.0));
        }
        (
            UiNodeRegionAlignment::PercentageFromMinimum,
            UiNodeRegionAlignment::PercentageFromMaximum,
        ) => {
            node.left = percent(x);
            node.right = percent(width.max(0.0));
        }
        (UiNodeRegionAlignment::PercentageFromMinimum, UiNodeRegionAlignment::Length) => {
            node.left = percent(x);
            node.width = px(width.max(0.0));
        }
        (UiNodeRegionAlignment::Minimum, UiNodeRegionAlignment::PercentageFromMinimum) => {
            node.left = px(x);
            node.right = percent((100.0 - width).max(0.0));
        }
        (UiNodeRegionAlignment::Minimum, UiNodeRegionAlignment::PercentageFromMaximum) => {
            node.left = px(x);
            node.right = percent(width.max(0.0));
        }
        (UiNodeRegionAlignment::Minimum, UiNodeRegionAlignment::Minimum) => {
            node.left = px(x);
            node.width = px((width - x).max(0.0));
        }
        (UiNodeRegionAlignment::Middle, UiNodeRegionAlignment::Length) => {
            node.left = percent(50.0);
            node.margin.left = px(x);
            node.width = px(width.max(0.0));
        }
        (UiNodeRegionAlignment::Middle, UiNodeRegionAlignment::Maximum) => {
            node.left = percent(50.0);
            node.margin.left = px(x);
            node.right = px(-width);
        }
        (UiNodeRegionAlignment::Minimum, UiNodeRegionAlignment::Maximum) => {
            node.left = px(x);
            node.right = px(-width);
        }
        (UiNodeRegionAlignment::Maximum, UiNodeRegionAlignment::Length) => {
            node.right = px(-(x + width));
            node.width = px(width.max(0.0));
        }
        (UiNodeRegionAlignment::Maximum, UiNodeRegionAlignment::Maximum) => {
            node.right = px(-width);
            node.width = px((width - x).max(0.0));
        }
        _ => {
            node.left = px(x);
            node.width = px(width.max(0.0));
        }
    }
}

fn apply_vertical_region(
    node: &mut Node,
    y: f32,
    height: f32,
    y_alignment: &UiNodeRegionAlignment,
    height_alignment: &UiNodeRegionAlignment,
) {
    match (y_alignment, height_alignment) {
        (
            UiNodeRegionAlignment::PercentageFromMinimum,
            UiNodeRegionAlignment::PercentageFromMinimum,
        ) => {
            node.top = percent(y);
            node.bottom = percent((100.0 - height).max(0.0));
        }
        (
            UiNodeRegionAlignment::PercentageFromMinimum,
            UiNodeRegionAlignment::PercentageFromMaximum,
        ) => {
            node.top = percent(y);
            node.bottom = percent(height.max(0.0));
        }
        (UiNodeRegionAlignment::PercentageFromMinimum, UiNodeRegionAlignment::Length) => {
            node.top = percent(y);
            node.height = px(height.max(0.0));
        }
        (UiNodeRegionAlignment::Minimum, UiNodeRegionAlignment::PercentageFromMinimum) => {
            node.top = px(y);
            node.bottom = percent((100.0 - height).max(0.0));
        }
        (UiNodeRegionAlignment::Minimum, UiNodeRegionAlignment::PercentageFromMaximum) => {
            node.top = px(y);
            node.bottom = percent(height.max(0.0));
        }
        (UiNodeRegionAlignment::Minimum, UiNodeRegionAlignment::Minimum) => {
            node.top = px(y);
            node.height = px((height - y).max(0.0));
        }
        (UiNodeRegionAlignment::Middle, UiNodeRegionAlignment::Length) => {
            node.top = percent(50.0);
            node.margin.top = px(y);
            node.height = px(height.max(0.0));
        }
        (UiNodeRegionAlignment::Middle, UiNodeRegionAlignment::Maximum) => {
            node.top = percent(50.0);
            node.margin.top = px(y);
            node.bottom = px(-height);
        }
        (UiNodeRegionAlignment::Minimum, UiNodeRegionAlignment::Maximum) => {
            node.top = px(y);
            node.bottom = px(-height);
        }
        (UiNodeRegionAlignment::Maximum, UiNodeRegionAlignment::Length) => {
            node.bottom = px(-(y + height));
            node.height = px(height.max(0.0));
        }
        (UiNodeRegionAlignment::Maximum, UiNodeRegionAlignment::Maximum) => {
            node.bottom = px(-height);
            node.height = px((height - y).max(0.0));
        }
        _ => {
            node.top = px(y);
            node.height = px(height.max(0.0));
        }
    }
}
