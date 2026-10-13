use bevy::{prelude::*, text::TextLayoutInfo, window::PrimaryWindow};
use openzt2_game_data::ui_document::{
    node_property_binding::UiTextPropertyBindingSource,
    widget_live_collection::UiWidgetLiveCollectionSource,
};

use openzt2_interface_state::persistence::profile_types::ProfileIndex;
use openzt2_interface_state::shell::shell_selection_types::ProfileChoice;
use openzt2_interface_state::shell::shell_selection_types::ShellScreen;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;
use openzt2_ui::authored_ui_text_layout_presentation::UiSingleLineTextAlignment;

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
};

type VerificationProfileRowPresentationQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static UiListRow,
        &'static ProfileChoice,
        &'static Text,
        &'static TextColor,
        &'static TextLayoutInfo,
        &'static ComputedNode,
        &'static UiGlobalTransform,
        &'static InheritedVisibility,
        Option<&'static CalculatedClip>,
    ),
>;

/// Entity presence alone cannot establish that a profile label is drawn in its list.
pub(super) fn collect_profile_list_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    index: Res<ProfileIndex>,
    rows: VerificationProfileRowPresentationQuery,
    lists: Query<(
        &UiListPolicy,
        &ComputedNode,
        &UiGlobalTransform,
        &InheritedVisibility,
    )>,
) {
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    let visible_populated_rows = rows
        .iter()
        .filter(
            |(row, choice, text, color, layout, node, transform, visibility, clip)| {
                let Some(profile) = index.profile_records.get(usize::from(row.index)) else {
                    return false;
                };
                let Ok((policy, list_node, list_transform, list_visibility)) = lists.get(row.list)
                else {
                    return false;
                };
                if policy.source != UiWidgetLiveCollectionSource::ProfileIndex
                    || !visibility.get()
                    || !list_visibility.get()
                    || node.is_empty()
                    || list_node.is_empty()
                    || color.0.alpha() <= 0.0
                    || text.0.is_empty()
                    || text.0 != profile.profile_display_name
                    || choice.0 != profile.profile_identifier
                    || layout.glyphs.is_empty()
                    || layout.size.min_element() <= 0.0
                {
                    return false;
                }
                let content = node.content_box();
                let list_bounds = Rect::from_corners(
                    list_transform
                        .affine()
                        .transform_point2(-list_node.size() * 0.5),
                    list_transform
                        .affine()
                        .transform_point2(list_node.size() * 0.5),
                );
                // Check glyph quads, not line height: the authored row's advance
                // need not include the font's entire line box. Rows are axis-aligned.
                layout.size.x <= content.size().x
                    && layout.glyphs.iter().all(|glyph| {
                        let local_bounds = Rect::from_center_size(
                            content.min + glyph.position,
                            glyph.atlas_info.rect.size(),
                        );
                        let glyph_bounds = Rect::from_corners(
                            transform.affine().transform_point2(local_bounds.min),
                            transform.affine().transform_point2(local_bounds.max),
                        );
                        list_bounds.contains(glyph_bounds.min)
                            && list_bounds.contains(glyph_bounds.max)
                            && clip.is_none_or(|clip| {
                                clip.clip.contains(glyph_bounds.min)
                                    && clip.clip.contains(glyph_bounds.max)
                            })
                    })
            },
        )
        .count();
    report.current_facts.insert(
        "profile_visible_populated_rows".to_owned(),
        f64::from(u32::try_from(visible_populated_rows).unwrap_or(u32::MAX)),
    );
}

type VerificationMainMenuProfileTextQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static UiDocumentOwner,
        &'static Text,
        &'static TextColor,
        &'static TextLayoutInfo,
        &'static ComputedNode,
        &'static UiGlobalTransform,
        &'static InheritedVisibility,
        Option<&'static UiTextBinding>,
        Option<&'static UiSingleLineTextAlignment>,
        Option<&'static CalculatedClip>,
    ),
>;

pub(super) fn collect_main_menu_profile_text_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    index: Res<ProfileIndex>,
    texts: VerificationMainMenuProfileTextQuery,
    roots: Query<&ChildOf, With<UiDocumentRoot>>,
    screens: Query<&ShellScreen>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    let selected_profile = index
        .profile_records
        .iter()
        .find(|profile| Some(profile.profile_identifier) == index.selected_profile_identifier);
    let viewport = windows
        .single()
        .ok()
        .map(|window| Rect::from_corners(Vec2::ZERO, window.physical_size().as_vec2()));
    let mut visible_name_owners = 0_u32;
    let mut readable_bound_names = 0_u32;
    if let (Some(profile), Some(viewport)) = (selected_profile, viewport) {
        for (owner, text, color, layout, node, transform, visibility, binding, alignment, clip) in
            &texts
        {
            if !roots
                .get(owner.0)
                .ok()
                .and_then(|parent| screens.get(parent.parent()).ok())
                .is_some_and(|screen| *screen == ShellScreen::MainMenu)
                || !visibility.get()
                || node.is_empty()
                || color.0.alpha() <= 0.0
                || text.0.is_empty()
                || text.0 != profile.profile_display_name
                || layout.glyphs.is_empty()
            {
                continue;
            }
            let content = node.content_box();
            let origin = content.min
                + Vec2::new(
                    alignment.map_or(0.0, |alignment| alignment.horizontal_offset_pixels),
                    0.0,
                );
            let visible_bounds = clip.map_or(viewport, |clip| viewport.intersect(clip.clip));
            let mut any_visible_glyph = false;
            let mut all_glyphs_contained = true;
            for glyph in &layout.glyphs {
                let local =
                    Rect::from_center_size(origin + glyph.position, glyph.atlas_info.rect.size());
                let glyph_bounds = Rect::from_corners(
                    transform.affine().transform_point2(local.min),
                    transform.affine().transform_point2(local.max),
                );
                any_visible_glyph |= !glyph_bounds.intersect(visible_bounds).is_empty();
                all_glyphs_contained &= visible_bounds.contains(glyph_bounds.min)
                    && visible_bounds.contains(glyph_bounds.max);
            }
            // Count any matching draw owner, including an unbound duplicate.
            // A clipped duplicate must not disappear from the ownership check.
            visible_name_owners += u32::from(any_visible_glyph);
            readable_bound_names += u32::from(
                all_glyphs_contained
                    && binding.is_some_and(|binding| {
                        binding.0 == UiTextPropertyBindingSource::ProfileName
                    }),
            );
        }
    }
    for (name, count) in [
        ("main_menu_profile_visible_text_owners", visible_name_owners),
        (
            "main_menu_profile_readable_bound_names",
            readable_bound_names,
        ),
    ] {
        report
            .current_facts
            .insert(name.to_owned(), f64::from(count));
    }
}
