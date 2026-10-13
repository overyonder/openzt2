use bevy::{
    prelude::*,
    text::TextLayoutInfo,
    ui::{CalculatedClip, UiGlobalTransform},
    window::PrimaryWindow,
};

use crate::plugins::mod_manager::mod_manager_types::{
    AssetArchiveEnabledStateCheckbox, AssetArchiveReloadState,
};
use openzt2_game_assets::asset_source::AssetArchives;

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
};

type ArchiveLabelQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Text,
        &'static TextColor,
        &'static TextLayoutInfo,
        &'static ComputedNode,
        &'static UiGlobalTransform,
        &'static InheritedVisibility,
        Option<&'static CalculatedClip>,
    ),
>;

pub(super) fn collect_mod_manager_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    archives: Res<AssetArchives>,
    reload: Res<AssetArchiveReloadState>,
    rows: Query<&Children, With<AssetArchiveEnabledStateCheckbox>>,
    labels: ArchiveLabelQuery,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    let statuses = archives.enabled_archive_statuses();
    // Observe canonical names, priority order and flags; retain no archive registry.
    let mut hash = blake3::Hasher::new();
    for status in &statuses {
        let name = status.archive_name.as_os_str().as_encoded_bytes();
        hash.update(&name.len().to_le_bytes());
        hash.update(name);
        hash.update(&[
            u8::from(status.is_enabled),
            u8::from(status.is_required_base_archive),
        ]);
    }
    for (index, bytes) in hash.finalize().as_bytes()[..16].chunks_exact(4).enumerate() {
        let value = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        report
            .current_facts
            .insert(format!("mod_archive_state_{index}"), f64::from(value));
    }
    let viewport = windows
        .single()
        .ok()
        .map(|window| Rect::from_corners(Vec2::ZERO, window.physical_size().as_vec2()));
    let visible_labels = rows
        .iter()
        .flat_map(|children| children.iter())
        .filter(|child| {
            let Ok((text, color, layout, node, transform, visibility, clip)) = labels.get(*child)
            else {
                return false;
            };
            if !visibility.get()
                || node.is_empty()
                || text.0.is_empty()
                || color.0.alpha() <= 0.0
                || layout.glyphs.is_empty()
            {
                return false;
            }
            let content = node.content_box();
            let bounds = Rect::from_corners(
                transform.transform_point2(content.min),
                transform.transform_point2(content.max),
            );
            viewport.is_some_and(|viewport| {
                viewport.contains(bounds.min) && viewport.contains(bounds.max)
            }) && clip
                .is_none_or(|clip| clip.clip.contains(bounds.min) && clip.clip.contains(bounds.max))
        })
        .count();
    for (name, count) in [
        ("mod_archive_count", statuses.len()),
        ("mod_manager_archive_rows", rows.iter().count()),
        ("mod_manager_visible_archive_labels", visible_labels),
    ] {
        report.current_facts.insert(
            name.to_owned(),
            f64::from(u32::try_from(count).unwrap_or(u32::MAX)),
        );
    }
    report.current_facts.insert(
        "mod_archive_reload_in_progress".to_owned(),
        f64::from(u8::from(reload.is_in_progress())),
    );
}
