use bevy::{
    math::Affine2,
    prelude::*,
    render::Extract,
    text::TextLayoutInfo,
    ui::ComputedStackIndex,
    ui_render::{stack_z_offsets, ExtractedUiItem, ExtractedUiNodes},
    window::PrimaryWindow,
};

use super::{
    authored_ui_canvas_scaling_and_clipping::UiLogicalCanvas,
    authored_ui_text_layout_presentation::{UiPhysicalFontSize, UiSingleLineTextAlignment},
};

/// Preserve system-font aspect when the authored canvas stretches. Bevy owns
/// shaping, glyph atlases and drawing; node regions and images stay unchanged.
pub(super) fn preserve_authored_font_aspect_in_extracted_text_draws(
    windows: Extract<Query<&Window, With<PrimaryWindow>>>,
    hierarchy: Extract<Query<(Option<&UiLogicalCanvas>, Option<&ChildOf>)>>,
    text_nodes: Extract<
        Query<
            (
                &ComputedNode,
                &ComputedStackIndex,
                &UiGlobalTransform,
                &TextLayout,
                &TextLayoutInfo,
                Has<UiSingleLineTextAlignment>,
            ),
            (With<UiPhysicalFontSize>, With<Text>),
        >,
    >,
    mut extracted: ResMut<ExtractedUiNodes>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let viewport = Vec2::new(window.width(), window.height());
    for draw in &mut extracted.uinodes {
        let entity = draw.main_entity.id();
        let Ok((node, stack, transform, layout, glyphs, single_line)) = text_nodes.get(entity)
        else {
            continue;
        };
        if !matches!(draw.item, ExtractedUiItem::Glyphs { .. })
            && draw.z_order <= stack.0 as f32 + stack_z_offsets::MATERIAL
        {
            continue;
        }
        let mut ancestor = entity;
        let mut logical_size = None;
        while let Ok((canvas, parent)) = hierarchy.get(ancestor) {
            if let Some(canvas) = canvas {
                logical_size = Some(canvas.logical_size());
                break;
            }
            let Some(parent) = parent else {
                break;
            };
            ancestor = parent.parent();
        }
        let Some(logical_size) = logical_size else {
            continue;
        };
        if viewport.min_element() <= 0.0 || logical_size.min_element() <= 0.0 {
            continue;
        }
        let alignment = match layout.justify {
            Justify::Center => 0.5,
            Justify::End | Justify::Right => 1.0,
            _ => 0.0,
        };
        let width = if single_line {
            glyphs.size.x
        } else {
            node.content_box().size().x
        };
        let anchor = node.content_box().min + Vec2::new(width * alignment, 0.0);
        let global = transform.affine();
        if global.matrix2.determinant().abs() <= f32::EPSILON {
            continue;
        }
        let correction = global
            * Affine2::from_translation(anchor)
            * Affine2::from_scale(Vec2::new(
                viewport.y * logical_size.x / (logical_size.y * viewport.x),
                1.0,
            ))
            * Affine2::from_translation(-anchor)
            * global.inverse();
        draw.transform = correction * draw.transform;
    }
}
