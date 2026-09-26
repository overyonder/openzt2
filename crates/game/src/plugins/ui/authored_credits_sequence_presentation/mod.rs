use bevy::prelude::*;

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

use super::authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiNodeId};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiAuthoredCreditsSequencePlayback {
    source_node_index: u32,
    repeat_after_milliseconds: u32,
    elapsed_milliseconds: u32,
}

impl UiAuthoredCreditsSequencePlayback {
    pub(super) fn from_authored_sequence(
        source_node_index: u32,
        repeat_after_milliseconds: u32,
    ) -> Self {
        Self {
            source_node_index,
            repeat_after_milliseconds,
            elapsed_milliseconds: 0,
        }
    }
}

/// Advances the source-authored credits timeline over ordinary projected UI
/// entities. Cards remain the authored nodes; the only live state is the
/// elapsed time on this one playback component.
pub(super) fn advance_authored_credits_sequence_presentations(
    time: Res<Time>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    mut playbacks: Query<(&UiDocumentOwner, &mut UiAuthoredCreditsSequencePlayback)>,
    mut nodes: Query<(&UiNodeId, &UiDocumentOwner, &mut Visibility)>,
) {
    let elapsed_frame_milliseconds = time.delta().as_millis().min(u128::from(u32::MAX)) as u32;
    for (owner, mut playback) in &mut playbacks {
        let Ok(root) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let Some(openzt2_game_data::ui_document::widget::UiWidgetRecord::CreditsSequence {
            cards,
            ..
        }) = document
            .canonical_ui_document()
            .nodes
            .get(playback.source_node_index as usize)
            .map(|node| &node.widget)
        else {
            continue;
        };
        let presentation_duration_milliseconds =
            cards.iter().map(|card| card.hide_at_ms).max().unwrap_or(0);
        let cycle_duration_milliseconds = presentation_duration_milliseconds
            .saturating_add(playback.repeat_after_milliseconds)
            .max(1);
        playback.elapsed_milliseconds = playback
            .elapsed_milliseconds
            .saturating_add(elapsed_frame_milliseconds)
            % cycle_duration_milliseconds;

        for card in cards {
            let card_is_visible = playback.elapsed_milliseconds >= card.show_at_ms
                && playback.elapsed_milliseconds < card.hide_at_ms;
            for (id, candidate_owner, mut visibility) in &mut nodes {
                if (candidate_owner.0, id.id.0) != (owner.0, card.node.0) {
                    continue;
                }
                let next_visibility = if card_is_visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if *visibility != next_visibility {
                    *visibility = next_visibility;
                }
            }
        }
    }
}
