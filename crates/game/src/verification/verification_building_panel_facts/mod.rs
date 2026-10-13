use bevy::{prelude::*, text::TextLayoutInfo, window::PrimaryWindow};
use openzt2_game_data::ui_document::node_style::UiStyleFlags;

use crate::plugins::{
    economy::{
        facility_economy_types::{MonthlyUpkeep, OperatingSinceDay, ServiceFacility},
        finance_table_ui::write_whole_dollar_currency_with_thousands_separators,
    },
    information::entity_selection_types::InfoPanel,
    ui::{
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiNodeId},
        authored_ui_text_layout_presentation::UiSingleLineTextAlignment,
    },
};
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
};

type VerificationBuildingFieldQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static UiDocumentOwner,
        &'static UiNodeId,
        &'static ComputedNode,
        &'static UiGlobalTransform,
        &'static InheritedVisibility,
        Option<&'static Text>,
        Option<&'static TextLayoutInfo>,
        Option<&'static TextColor>,
        Option<&'static UiSingleLineTextAlignment>,
        Option<&'static CalculatedClip>,
    ),
>;

pub(super) fn collect_building_panel_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    panels: Query<(Entity, &InfoPanel, &UiDocumentRoot, &InheritedVisibility)>,
    documents: Res<Assets<UiDocumentAsset>>,
    fields: VerificationBuildingFieldQuery,
    subjects: Query<(
        Option<&MonthlyUpkeep>,
        Has<OperatingSinceDay>,
        Has<ServiceFacility>,
    )>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    let mut contained_fields = 0_u32;
    let mut readable_upkeep = false;
    let mut operating_age_available = false;
    let mut current_users_available = false;
    let mut frame_within_viewport = false;
    let viewport = windows
        .single()
        .ok()
        .map(|window| Rect::from_corners(Vec2::ZERO, window.physical_size().as_vec2()));
    for (panel_entity, panel, root, visible) in &panels {
        if !visible.get() {
            continue;
        }
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let data = document.canonical_ui_document();
        let named_field = |owner: &UiDocumentOwner, id: &UiNodeId, name: &str| {
            owner.0 == panel_entity && data.nodes[id.index as usize].name == name
        };
        if !fields.iter().any(|(owner, id, _, _, visibility, ..)| {
            visibility.get() && named_field(owner, id, "Building Info")
        }) {
            continue;
        }
        let Some(background_index) = data
            .nodes
            .iter()
            .position(|node| node.name == "Backgrounds")
        else {
            continue;
        };
        // The section's 275-unit extent includes empty space below the panel.
        // Its largest authored backdrop is the visible owning frame.
        let Some((frame_bounds, frame_clip)) = fields
            .iter()
            .filter(|(owner, id, node, _, visibility, ..)| {
                if owner.0 != panel_entity {
                    return false;
                }
                let record = &data.nodes[id.index as usize];
                visibility.get()
                    && !node.is_empty()
                    && record.parent as usize == background_index
                    && record.style.flags.0 & UiStyleFlags::NINE_SLICE.0 != 0
            })
            .map(|(_, _, node, transform, _, _, _, _, _, clip)| {
                (transformed_building_field_bounds(node, transform), clip)
            })
            .max_by(|(left, _), (right, _)| {
                left.size()
                    .element_product()
                    .total_cmp(&right.size().element_product())
            })
        else {
            continue;
        };
        frame_within_viewport = viewport.is_some_and(|viewport| {
            viewport.contains(frame_bounds.min)
                && viewport.contains(frame_bounds.max)
                && frame_clip.is_none_or(|clip| {
                    clip.clip.contains(frame_bounds.min) && clip.clip.contains(frame_bounds.max)
                })
        });
        let Ok((upkeep, opened, facility)) = subjects.get(panel.subject) else {
            continue;
        };
        operating_age_available = opened;
        current_users_available = facility;
        let mut expected_upkeep = String::new();
        if let Some(upkeep) = upkeep {
            write_whole_dollar_currency_with_thousands_separators(
                &mut expected_upkeep,
                upkeep.0 .0,
            );
        }
        for (owner, id, node, transform, visibility, text, layout, color, alignment, clip) in
            &fields
        {
            if owner.0 != panel_entity || !visibility.get() || node.is_empty() {
                continue;
            }
            let name = data.nodes[id.index as usize].name.as_str();
            if !matches!(
                name,
                "Month Number" | "Current Users" | "Current Upkeep" | "Lifetime Num"
            ) {
                continue;
            }
            let bounds = transformed_building_field_bounds(node, transform);
            if !frame_bounds.contains(bounds.min)
                || !frame_bounds.contains(bounds.max)
                || !viewport.is_some_and(|viewport| {
                    viewport.contains(bounds.min) && viewport.contains(bounds.max)
                })
                || clip.is_some_and(|clip| {
                    !clip.clip.contains(bounds.min) || !clip.clip.contains(bounds.max)
                })
            {
                continue;
            }
            contained_fields += 1;
            if name != "Current Upkeep"
                || expected_upkeep.is_empty()
                || !text.is_some_and(|text| text.0 == expected_upkeep)
                || !color.is_some_and(|color| color.0.alpha() > 0.0)
            {
                continue;
            }
            let Some(layout) = layout else {
                continue;
            };
            readable_upkeep =
                building_field_glyphs_fit_bounds(node, transform, layout, alignment, clip, bounds);
        }
    }
    for (name, value) in [
        (
            "building_panel_frame_within_viewport",
            f64::from(u8::from(frame_within_viewport)),
        ),
        (
            "building_value_fields_within_frame",
            f64::from(contained_fields),
        ),
        (
            "building_readable_upkeep_matches_owner",
            f64::from(u8::from(readable_upkeep)),
        ),
        (
            "building_operating_age_available",
            f64::from(u8::from(operating_age_available)),
        ),
        (
            "building_current_users_available",
            f64::from(u8::from(current_users_available)),
        ),
    ] {
        report.current_facts.insert(name.to_owned(), value);
    }
}

fn building_field_glyphs_fit_bounds(
    node: &ComputedNode,
    transform: &UiGlobalTransform,
    layout: &TextLayoutInfo,
    alignment: Option<&UiSingleLineTextAlignment>,
    clip: Option<&CalculatedClip>,
    bounds: Rect,
) -> bool {
    let origin = node.content_box().min
        + Vec2::new(
            alignment.map_or(0.0, |alignment| alignment.horizontal_offset_pixels),
            0.0,
        );
    !layout.glyphs.is_empty()
        && layout.glyphs.iter().all(|glyph| {
            let local =
                Rect::from_center_size(origin + glyph.position, glyph.atlas_info.rect.size());
            let minimum = transform.affine().transform_point2(local.min);
            let maximum = transform.affine().transform_point2(local.max);
            bounds.contains(minimum)
                && bounds.contains(maximum)
                && clip
                    .is_none_or(|clip| clip.clip.contains(minimum) && clip.clip.contains(maximum))
        })
}

fn transformed_building_field_bounds(node: &ComputedNode, transform: &UiGlobalTransform) -> Rect {
    Rect::from_corners(
        transform.affine().transform_point2(-node.size() * 0.5),
        transform.affine().transform_point2(node.size() * 0.5),
    )
}
