use bevy::{
    prelude::*,
    render::MainWorld,
    ui::ComputedStackIndex,
    ui_render::{stack_z_offsets, ExtractedUiItem, ExtractedUiNodes},
    window::PrimaryWindow,
};
use openzt2_game_data::ui_document::node_property_binding::UiTextPropertyBindingSource;

use crate::plugins::{
    scenario::scenario_objective_types::{ScenarioObjective, ScenarioObjectiveAuthoredVisibility},
    ui::authored_ui_text_content_binding::UiTextBinding,
};

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
};

/// Shown scenario objectives, and the goal and challenge-offer texts actually
/// drawn: a text counts only when its extracted glyphs all lie inside the
/// viewport and the node's clip. A row that exists but is laid out out of
/// sight, or a paragraph running past its box, does not count.
pub(super) fn collect_extracted_scenario_goal_verification_facts(
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
    let mut objectives =
        main.query_filtered::<&ScenarioObjectiveAuthoredVisibility, With<ScenarioObjective>>();
    let shown_objectives = objectives.iter(&main).filter(|shown| shown.0).count();
    let mut windows = main.query_filtered::<&Window, With<PrimaryWindow>>();
    let viewport = windows
        .single(&main)
        .ok()
        .map(|window| Rect::from_corners(Vec2::ZERO, window.physical_size().as_vec2()));
    let mut goal_texts = main.query::<(Entity, &UiTextBinding, &InheritedVisibility)>();
    let goal_text_owners = goal_texts
        .iter(&main)
        .filter(|(_, binding, visibility)| {
            visibility.get()
                && matches!(
                    binding.0,
                    UiTextPropertyBindingSource::ScenarioObjectiveText { .. }
                )
        })
        .map(|(entity, ..)| entity)
        .collect::<Vec<_>>();
    let mut named_texts = main.query::<(Entity, &Name, &InheritedVisibility)>();
    let mut named = |name: &str| {
        named_texts
            .iter(&main)
            .filter(|(_, candidate, visibility)| visibility.get() && candidate.as_str() == name)
            .map(|(entity, ..)| entity)
            .collect::<Vec<_>>()
    };
    let offer_text_owners = named("instant challenge text");
    let offer_description_owners = named("instant challenge description text");

    // Owners whose glyphs were all drawn inside the viewport and their clip.
    let drawn_within_clip = |owners: &[Entity]| {
        owners
            .iter()
            .filter(|owner| {
                let stack = main.get::<ComputedStackIndex>(**owner);
                let mut drawn = 0_u32;
                let mut transparent = 0_u32;
                let mut outside_viewport = 0_u32;
                let mut outside_clip = None;
                let mut text_draws = 0_u32;
                for draw in extracted.uinodes.iter().filter(|draw| {
                    draw.main_entity.id() == **owner
                        && matches!(draw.item, ExtractedUiItem::Glyphs { .. })
                }) {
                    if stack
                        .is_none_or(|stack| draw.z_order != stack.0 as f32 + stack_z_offsets::TEXT)
                    {
                        continue;
                    }
                    text_draws = text_draws.saturating_add(1);
                    let ExtractedUiItem::Glyphs { range } = &draw.item else {
                        continue;
                    };
                    for glyph in &extracted.glyphs[range.clone()] {
                        drawn = drawn.saturating_add(1);
                        if glyph.color.alpha <= 0.0 {
                            transparent = transparent.saturating_add(1);
                        }
                        let rect = Rect::from_center_size(glyph.translation, glyph.rect.size());
                        for corner in [
                            rect.min,
                            Vec2::new(rect.min.x, rect.max.y),
                            rect.max,
                            Vec2::new(rect.max.x, rect.min.y),
                        ] {
                            let point = draw.transform.transform_point2(corner);
                            if !viewport.is_some_and(|viewport| viewport.contains(point)) {
                                outside_viewport = outside_viewport.saturating_add(1);
                            }
                            if draw.clip.is_some_and(|clip| !clip.contains(point))
                                && outside_clip.is_none()
                            {
                                outside_clip = Some((point, draw.clip));
                            }
                        }
                    }
                }
                let glyph_draws = extracted
                    .uinodes
                    .iter()
                    .filter(|draw| {
                        draw.main_entity.id() == **owner
                            && matches!(draw.item, ExtractedUiItem::Glyphs { .. })
                    })
                    .map(|draw| draw.z_order)
                    .collect::<Vec<_>>();
                debug!(
                    target: "openzt2_verification::scenario_text_layout",
                    owner = ?**owner,
                    stack = ?stack.map(|stack| stack.0),
                    ?glyph_draws,
                    text_draws,
                    drawn,
                    transparent,
                    outside_viewport,
                    ?outside_clip,
                    ?viewport,
                    "drawn text fact"
                );
                drawn > 0 && transparent == 0 && outside_viewport == 0 && outside_clip.is_none()
            })
            .count()
    };
    let count = |count: usize| f64::from(u32::try_from(count).unwrap_or(u32::MAX));
    let facts = [
        ("shown_scenario_objectives", count(shown_objectives)),
        (
            "drawn_scenario_goal_texts",
            count(drawn_within_clip(&goal_text_owners)),
        ),
        (
            "drawn_instant_challenge_texts",
            count(drawn_within_clip(&offer_text_owners)),
        ),
        (
            "drawn_instant_challenge_descriptions",
            count(drawn_within_clip(&offer_description_owners)),
        ),
    ];
    main.resource_mut::<VerificationJourneyReport>()
        .current_facts
        .extend(facts.map(|(name, value)| (name.to_owned(), value)));
}
