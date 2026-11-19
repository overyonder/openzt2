use bevy::{
    math::Affine2,
    prelude::*,
    render::Extract,
    ui::ComputedStackIndex,
    ui_render::{stack_z_offsets, ExtractedUiItem, ExtractedUiNodes},
};

use super::{
    authored_ui_canvas_scaling_and_clipping::{UiCanvasMembership, UiCanvasMetrics},
    authored_ui_text_layout_presentation::{UiPhysicalFontSize, UiSingleLineTextAlignment},
};

/// Preserve system-font aspect when the authored canvas stretches. Bevy owns
/// shaping, glyph atlases and drawing; node regions and images stay unchanged.
pub(super) fn preserve_authored_font_aspect_in_extracted_text_draws(
    parents: Extract<Query<&ChildOf>>,
    canvas_members: Extract<Query<&UiCanvasMembership>>,
    canvases: Extract<Query<&UiCanvasMetrics>>,
    text_nodes: Extract<
        Query<
            (
                &ComputedNode,
                &ComputedStackIndex,
                &UiGlobalTransform,
                &TextLayout,
                Option<&UiSingleLineTextAlignment>,
            ),
            (With<UiPhysicalFontSize>, With<Text>),
        >,
    >,
    extracted: ResMut<ExtractedUiNodes>,
) {
    let ExtractedUiNodes { uinodes, changed } = extracted.into_inner();
    for main_entity in changed.iter() {
        let Some((_, draws)) = uinodes.get_mut(main_entity) else {
            continue;
        };
        let entity = main_entity.entity();
        // Rich tooltip text draws from a child laid over its single-line
        // owner's transparent fallback, so it follows the owner's geometry.
        let Some((node, stack, transform, layout, single_line)) =
            text_nodes.get(entity).ok().or_else(|| {
                let parent = parents.get(entity).ok()?;
                text_nodes
                    .get(parent.parent())
                    .ok()
                    .filter(|owner| owner.4.is_some())
            })
        else {
            continue;
        };
        let Some(metrics) = canvas_members
            .get(entity)
            .ok()
            .and_then(|membership| canvases.get(membership.0).ok())
            .filter(|metrics| metrics.has_extent())
        else {
            continue;
        };
        let alignment = match layout.justify {
            Justify::Center => 0.5,
            Justify::End | Justify::Right => 1.0,
            _ => 0.0,
        };
        let text_offset = Vec2::new(
            single_line.map_or(0.0, |alignment| alignment.horizontal_offset_pixels),
            0.0,
        );
        let anchor = single_line.map_or_else(
            || node.content_box().min + Vec2::new(node.content_box().width() * alignment, 0.0),
            |line| line.anchor,
        );
        let global = transform.affine();
        if global.matrix2.determinant().abs() <= f32::EPSILON {
            continue;
        }
        let correction = global
            * Affine2::from_translation(anchor)
            * Affine2::from_scale(metrics.isotropic_correction)
            * Affine2::from_translation(-anchor)
            * global.inverse();
        let correction =
            correction * Affine2::from_translation(global.transform_vector2(text_offset));
        for draw in draws.values_mut() {
            if matches!(draw.item, ExtractedUiItem::Glyphs { .. })
                || draw.z_order > stack.0 as f32 + stack_z_offsets::MATERIAL
            {
                draw.transform = correction * draw.transform;
            }
        }
    }
}
