use bevy::{
    prelude::*,
    render::MainWorld,
    ui_render::{ExtractedUiItem, ExtractedUiNodes},
    window::PrimaryWindow,
};
use openzt2_game_data::ui_document::node_property_binding::UiTextPropertyBindingSource;

use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
};

/// Observe canonical title/body text, including the body's rich-text descendants.
/// Extracted glyphs inside the clip still require independent pixel review.
pub(super) fn collect_extracted_zoopedia_verification_facts(
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
    let mut fields = main.query::<(&UiTextBinding, &InheritedVisibility)>();
    let titles = fields
        .iter(&main)
        .filter(|(binding, visibility)| {
            visibility.get() && matches!(binding.0, UiTextPropertyBindingSource::ZoopediaTitle)
        })
        .count();
    let mut title_glyphs = 0_u32;
    let mut body_glyphs = 0_u32;
    for draw in &extracted.uinodes {
        let ExtractedUiItem::Glyphs { range } = &draw.item else {
            continue;
        };
        let binding = std::iter::successors(Some(draw.main_entity.id()), |entity| {
            main.get::<ChildOf>(*entity).map(ChildOf::parent)
        })
        .find_map(|entity| {
            main.get::<UiTextBinding>(entity)
                .map(|binding| &binding.0)
                .filter(|binding| {
                    matches!(
                        binding,
                        UiTextPropertyBindingSource::ZoopediaTitle
                            | UiTextPropertyBindingSource::ZoopediaBody
                    )
                })
        });
        let Some(binding) = binding else {
            continue;
        };
        for glyph in &extracted.glyphs[range.clone()] {
            let rect = Rect::from_center_size(glyph.translation, glyph.rect.size());
            let contained = [
                rect.min,
                Vec2::new(rect.min.x, rect.max.y),
                rect.max,
                Vec2::new(rect.max.x, rect.min.y),
            ]
            .into_iter()
            .all(|corner| {
                let point = draw.transform.transform_point2(corner);
                viewport.is_some_and(|viewport| viewport.contains(point))
                    && draw.clip.is_none_or(|clip| clip.contains(point))
            });
            if glyph.color.alpha > 0.0 && glyph.rect.size().cmpgt(Vec2::ZERO).all() && contained {
                if matches!(binding, UiTextPropertyBindingSource::ZoopediaTitle) {
                    title_glyphs = title_glyphs.saturating_add(1);
                } else {
                    body_glyphs = body_glyphs.saturating_add(1);
                }
            }
        }
    }
    main.resource_mut::<VerificationJourneyReport>()
        .current_facts
        .extend([
            (
                "zoopedia_visible_title_owners".to_owned(),
                f64::from(u32::try_from(titles).unwrap_or(u32::MAX)),
            ),
            (
                "zoopedia_visible_title_glyphs".to_owned(),
                f64::from(title_glyphs),
            ),
            (
                "zoopedia_visible_body_glyphs".to_owned(),
                f64::from(body_glyphs),
            ),
        ]);
}
