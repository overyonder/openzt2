use bevy::{
    prelude::*,
    render::MainWorld,
    text::TextLayoutInfo,
    ui::ComputedStackIndex,
    ui_render::{stack_z_offsets, ExtractedUiItem, ExtractedUiNodes},
    window::PrimaryWindow,
};
use openzt2_game_data::ui_document::node_property_binding::UiTextPropertyBindingSource;

use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
};

/// Inspect the bound sale fields after text extraction and canvas correction.
/// Extracted glyphs do not establish atlas pixels or final occlusion.
pub(super) fn collect_extracted_sale_confirmation_verification_facts(
    mut main: ResMut<MainWorld>,
    extracted: Res<ExtractedUiNodes>,
) {
    if main
        .resource::<VerificationJourneySession>()
        .fact_collection
        != VerificationFactCollectionState::Collected
    {
        return;
    }
    let mut windows = main.query_filtered::<&Window, With<PrimaryWindow>>();
    let viewport = windows
        .single(&main)
        .ok()
        .map(|window| Rect::from_corners(Vec2::ZERO, window.physical_size().as_vec2()));
    let mut fields = main.query::<(Entity, &UiTextBinding, &InheritedVisibility)>();
    let mut facts = Vec::new();
    for (question, prefix) in [(true, "sale_question"), (false, "sale_refund")] {
        let owners = fields
            .iter(&main)
            .filter_map(|(entity, binding, visibility)| {
                (visibility.get()
                    && matches!(
                        (&binding.0, question),
                        (
                            UiTextPropertyBindingSource::SelectedEntitySaleConfirmation,
                            true
                        ) | (
                            UiTextPropertyBindingSource::SelectedEntitySaleRefund { .. },
                            false
                        )
                    ))
                .then_some(entity)
            })
            .collect::<Vec<_>>();
        let computed_size = owners
            .iter()
            .map(|owner| {
                main.get::<ComputedNode>(*owner)
                    .map_or(Vec2::ZERO, ComputedNode::size)
            })
            .reduce(Vec2::min)
            .unwrap_or(Vec2::ZERO);
        let mut measured = 0_u32;
        let mut drawn = 0_u32;
        let mut opacity = 1.0_f32;
        let mut contained = true;
        for owner in &owners {
            if let Some(layout) = main.get::<TextLayoutInfo>(*owner) {
                measured =
                    measured.saturating_add(u32::try_from(layout.glyphs.len()).unwrap_or(u32::MAX));
            }
            let Some(stack) = main.get::<ComputedStackIndex>(*owner) else {
                continue;
            };
            for draw in extracted.uinodes.iter().filter(|draw| {
                draw.main_entity.id() == *owner
                    && draw.z_order == stack.0 as f32 + stack_z_offsets::TEXT
            }) {
                let ExtractedUiItem::Glyphs { range } = &draw.item else {
                    continue;
                };
                for glyph in &extracted.glyphs[range.clone()] {
                    drawn = drawn.saturating_add(1);
                    opacity = opacity.min(glyph.color.alpha);
                    let rect = Rect::from_center_size(glyph.translation, glyph.rect.size());
                    for corner in [
                        rect.min,
                        Vec2::new(rect.min.x, rect.max.y),
                        rect.max,
                        Vec2::new(rect.max.x, rect.min.y),
                    ] {
                        let point = draw.transform.transform_point2(corner);
                        contained &= viewport.is_some_and(|viewport| viewport.contains(point))
                            && draw.clip.is_none_or(|clip| clip.contains(point));
                    }
                }
            }
        }
        facts.extend([
            (
                format!("{prefix}_visible_text_owners"),
                f64::from(u32::try_from(owners.len()).unwrap_or(u32::MAX)),
            ),
            (
                format!("{prefix}_layout_width_pixels"),
                f64::from(computed_size.x),
            ),
            (
                format!("{prefix}_layout_height_pixels"),
                f64::from(computed_size.y),
            ),
            (format!("{prefix}_measured_glyphs"), f64::from(measured)),
            (format!("{prefix}_extracted_glyphs"), f64::from(drawn)),
            (
                format!("{prefix}_glyph_opacity"),
                f64::from(if drawn == 0 { 0.0 } else { opacity }),
            ),
            (
                format!("{prefix}_glyphs_within_viewport_and_clip"),
                f64::from(u8::from(drawn > 0 && contained)),
            ),
        ]);
    }
    main.resource_mut::<VerificationJourneyReport>()
        .current_facts
        .extend(facts);
}
