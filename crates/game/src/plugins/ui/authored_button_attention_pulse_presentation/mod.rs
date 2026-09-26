use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::authored_ui_node_projection_components::{UiDocumentOwner, UiNodeId};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiAuthoredButtonAttentionPulse {
    target_node: AssetId,
    hidden_milliseconds: u32,
    visible_milliseconds: u32,
    elapsed_milliseconds: u32,
}

impl UiAuthoredButtonAttentionPulse {
    pub(super) fn from_authored_cadence(
        target_node: AssetId,
        hidden_milliseconds: u32,
        visible_milliseconds: u32,
    ) -> Self {
        Self {
            target_node,
            hidden_milliseconds,
            visible_milliseconds,
            elapsed_milliseconds: 0,
        }
    }
}

/// Alternates target visibility at the authored button-pulser cadence.
pub(super) fn advance_authored_button_attention_pulse_presentations(
    time: Res<Time>,
    mut pulses: Query<(&UiDocumentOwner, &mut UiAuthoredButtonAttentionPulse)>,
    mut nodes: Query<(&UiNodeId, &UiDocumentOwner, &mut Visibility)>,
) {
    let elapsed_frame_milliseconds = time.delta().as_millis().min(u128::from(u32::MAX)) as u32;
    for (owner, mut pulse) in &mut pulses {
        let cycle_duration_milliseconds = pulse
            .hidden_milliseconds
            .saturating_add(pulse.visible_milliseconds)
            .max(1);
        pulse.elapsed_milliseconds = pulse
            .elapsed_milliseconds
            .saturating_add(elapsed_frame_milliseconds)
            % cycle_duration_milliseconds;
        let target_is_visible = pulse.elapsed_milliseconds >= pulse.hidden_milliseconds;
        for (id, candidate_owner, mut visibility) in &mut nodes {
            if (candidate_owner.0, id.id) != (owner.0, pulse.target_node) {
                continue;
            }
            let next_visibility = if target_is_visible {
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
