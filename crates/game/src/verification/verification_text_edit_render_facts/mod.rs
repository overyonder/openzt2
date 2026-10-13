use bevy::{
    math::Affine2,
    prelude::*,
    render::MainWorld,
    text::{EditableText, TextLayoutInfo},
    ui::{widget::TextScroll, ComputedStackIndex},
    ui_render::{
        stack_z_offsets, ui_texture_slice_pipeline::ExtractedUiTextureSlices, ExtractedUiItem,
        ExtractedUiNodes,
    },
};

use crate::plugins::ui::authored_ui_visual_types::{UiVisualBackgroundSlotOwner, UiVisualLayer};

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
};

/// Measure the actual extracted editor draws after authored background ordering.
/// These facts cannot establish atlas pixels, GPU submission or final occlusion.
pub(super) fn collect_extracted_named_text_edit_verification_facts(
    mut main: ResMut<MainWorld>,
    extracted: Res<ExtractedUiNodes>,
    sliced: Res<ExtractedUiTextureSlices>,
) {
    if main
        .resource::<VerificationJourneySession>()
        .fact_collection
        != VerificationFactCollectionState::Collected
    {
        return;
    }
    let mut facts = Vec::new();
    let mut editors = main.query_filtered::<(Entity, &Name), With<EditableText>>();
    for (name, prefix) in [
        ("userstring_zooname", "zoo_name_editor"),
        ("SaveName", "save_name_editor"),
    ] {
        let editor = editors.iter(&main).find_map(|(entity, candidate)| {
            (candidate.as_str() == name
                && main
                    .get::<InheritedVisibility>(entity)
                    .is_some_and(|visibility| visibility.get()))
            .then_some(entity)
        });
        for (suffix, value) in measure_extracted_editor(&main, editor, &extracted, &sliced) {
            facts.push((format!("{prefix}_{suffix}"), value));
        }
    }
    main.resource_mut::<VerificationJourneyReport>()
        .current_facts
        .extend(facts);
}

fn measure_extracted_editor(
    world: &World,
    editor: Option<Entity>,
    extracted: &ExtractedUiNodes,
    sliced: &ExtractedUiTextureSlices,
) -> Vec<(&'static str, f64)> {
    let node = editor.and_then(|entity| world.get::<ComputedNode>(entity));
    let transform = editor.and_then(|entity| world.get::<UiGlobalTransform>(entity));
    let layout = editor.and_then(|entity| world.get::<TextLayoutInfo>(entity));
    let stack = editor.and_then(|entity| world.get::<ComputedStackIndex>(entity));
    let bounds = node.zip(transform).map(|(node, transform)| {
        transformed_rect(
            transform.affine(),
            Rect::from_center_size(Vec2::ZERO, node.size()),
        )
    });
    // Match Bevy's primary text transform, excluding offset text-shadow draws.
    let text_transform = node.zip(transform).map(|(node, transform)| {
        let scroll = editor.and_then(|entity| world.get::<TextScroll>(entity));
        transform.affine()
            * Affine2::from_translation(
                node.content_box().min - scroll.map_or(Vec2::ZERO, |scroll| scroll.0),
            )
    });
    let mut glyph_count = 0_u32;
    let mut glyph_opacity = 1.0_f32;
    let mut glyph_bounds = None::<Rect>;
    let mut glyph_clip = None::<Rect>;
    let mut glyphs_in_editor = true;
    let mut glyphs_in_clip = true;
    let mut foreground_min_z = None::<f32>;
    let mut background_count = 0_u32;
    let mut background_max_z = None::<f32>;
    let mut caret_count = 0_u32;
    let mut caret_opacity = 0.0_f32;
    let mut caret_in_editor = false;
    let mut caret_in_clip = false;
    let mut selection_count = 0_u32;
    for draw in &extracted.uinodes {
        let entity = draw.main_entity.id();
        if editor.is_some_and(|editor| is_editor_background(world, entity, editor)) {
            if matches!(&draw.item, ExtractedUiItem::Node { color, .. } if color.alpha > 0.0) {
                background_count += 1;
                background_max_z =
                    Some(background_max_z.map_or(draw.z_order, |z| z.max(draw.z_order)));
            }
        }
        if Some(entity) != editor {
            continue;
        }
        match &draw.item {
            ExtractedUiItem::Glyphs { range } if text_transform == Some(draw.transform) => {
                if let Some(clip) = draw.clip {
                    glyph_clip = Some(glyph_clip.map_or(clip, |previous| previous.intersect(clip)));
                }
                foreground_min_z =
                    Some(foreground_min_z.map_or(draw.z_order, |z| z.min(draw.z_order)));
                for glyph in &extracted.glyphs[range.clone()] {
                    glyph_count += 1;
                    glyph_opacity = glyph_opacity.min(glyph.color.alpha);
                    let rect = transformed_rect(
                        draw.transform,
                        Rect::from_center_size(glyph.translation, glyph.rect.size()),
                    );
                    glyph_bounds = Some(glyph_bounds.map_or(rect, |bounds| bounds.union(rect)));
                    glyphs_in_editor &= bounds.is_some_and(|bounds| contains_rect(bounds, rect));
                    glyphs_in_clip &= draw.clip.is_none_or(|clip| contains_rect(clip, rect));
                }
            }
            ExtractedUiItem::Node { color, rect, .. } => {
                let Some(stack) = stack else {
                    continue;
                };
                if draw.z_order == stack.0 as f32 + stack_z_offsets::TEXT_CURSOR {
                    caret_count += 1;
                    caret_opacity = color.alpha;
                    let rect = transformed_rect(
                        draw.transform,
                        Rect::from_center_size(Vec2::ZERO, rect.size()),
                    );
                    caret_in_editor = bounds.is_some_and(|bounds| contains_rect(bounds, rect));
                    caret_in_clip = draw.clip.is_none_or(|clip| contains_rect(clip, rect));
                    foreground_min_z =
                        Some(foreground_min_z.map_or(draw.z_order, |z| z.min(draw.z_order)));
                } else if draw.z_order == stack.0 as f32 + stack_z_offsets::TEXT_SELECTION {
                    selection_count += 1;
                    foreground_min_z =
                        Some(foreground_min_z.map_or(draw.z_order, |z| z.min(draw.z_order)));
                } else if draw.z_order <= stack.0 as f32 + stack_z_offsets::MATERIAL
                    && color.alpha > 0.0
                {
                    background_count += 1;
                    background_max_z =
                        Some(background_max_z.map_or(draw.z_order, |z| z.max(draw.z_order)));
                }
            }
            _ => {}
        }
    }
    for draw in &sliced.slices {
        if editor.is_some_and(|editor| {
            is_editor_background(world, draw.main_entity.id(), editor)
                || draw.main_entity.id() == editor
        }) && draw.color.alpha > 0.0
        {
            background_count += 1;
            let z = draw.stack_index as f32 + stack_z_offsets::IMAGE;
            background_max_z = Some(background_max_z.map_or(z, |previous| previous.max(z)));
        }
    }
    let ordered = background_max_z
        .zip(foreground_min_z)
        .is_some_and(|(background, foreground)| background < foreground);
    let flag = |value| f64::from(u8::from(value));
    let mut facts = vec![
        (
            "measured_glyphs",
            layout.map_or(0.0, |layout| {
                f64::from(u32::try_from(layout.glyphs.len()).unwrap_or(u32::MAX))
            }),
        ),
        ("extracted_glyphs", f64::from(glyph_count)),
        (
            "glyph_opacity",
            if glyph_count == 0 {
                0.0
            } else {
                f64::from(glyph_opacity)
            },
        ),
        (
            "glyphs_within_editor",
            flag(glyph_count > 0 && glyphs_in_editor),
        ),
        (
            "glyphs_within_clip",
            flag(glyph_count > 0 && glyphs_in_clip),
        ),
        ("background_draws", f64::from(background_count)),
        (
            "background_max_z",
            f64::from(background_max_z.unwrap_or(0.0)),
        ),
        (
            "foreground_min_z",
            f64::from(foreground_min_z.unwrap_or(0.0)),
        ),
        ("background_below_foreground", flag(ordered)),
        ("extracted_caret", f64::from(caret_count)),
        ("caret_opacity", f64::from(caret_opacity)),
        ("caret_within_editor", flag(caret_in_editor)),
        ("caret_within_clip", flag(caret_in_clip)),
        (
            "caret_contained_and_nontransparent",
            flag(caret_count == 1 && caret_opacity > 0.0 && caret_in_editor && caret_in_clip),
        ),
        ("selection_draws", f64::from(selection_count)),
        ("glyph_has_clip", flag(glyph_clip.is_some())),
    ];
    if let Some(bounds) = glyph_bounds {
        facts.extend([
            ("glyph_min_x", f64::from(bounds.min.x)),
            ("glyph_min_y", f64::from(bounds.min.y)),
            ("glyph_max_x", f64::from(bounds.max.x)),
            ("glyph_max_y", f64::from(bounds.max.y)),
        ]);
    }
    if let Some(bounds) = bounds {
        facts.extend([
            ("bounds_min_x", f64::from(bounds.min.x)),
            ("bounds_min_y", f64::from(bounds.min.y)),
            ("bounds_max_x", f64::from(bounds.max.x)),
            ("bounds_max_y", f64::from(bounds.max.y)),
        ]);
    }
    if let Some(clip) = glyph_clip {
        facts.extend([
            ("clip_min_x", f64::from(clip.min.x)),
            ("clip_min_y", f64::from(clip.min.y)),
            ("clip_max_x", f64::from(clip.max.x)),
            ("clip_max_y", f64::from(clip.max.y)),
        ]);
    }
    facts
}

fn is_editor_background(world: &World, entity: Entity, editor: Entity) -> bool {
    world
        .get::<UiVisualBackgroundSlotOwner>(entity)
        .is_some_and(|owner| owner.0 == editor)
        || world.get::<UiVisualLayer>(entity).is_some()
            && world.get::<ChildOf>(entity).is_some_and(|parent| {
                parent.parent() == editor
                    || world
                        .get::<UiVisualBackgroundSlotOwner>(parent.parent())
                        .is_some_and(|owner| owner.0 == editor)
            })
}

fn transformed_rect(transform: Affine2, rect: Rect) -> Rect {
    Rect::from_corners(
        transform.transform_point2(rect.min),
        transform.transform_point2(rect.max),
    )
}

fn contains_rect(bounds: Rect, rect: Rect) -> bool {
    bounds.contains(rect.min) && bounds.contains(rect.max)
}
