use std::time::Duration;

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::authored_ui_node_projection_components::{UiDocumentOwner, UiNodeId};

#[derive(Component)]
pub(super) struct UiAuthoredButtonAttentionPulse {
    target_node: AssetId,
    hidden: Duration,
    phase: Timer,
}

impl UiAuthoredButtonAttentionPulse {
    pub(super) fn from_authored_cadence(
        target_node: AssetId,
        hidden_milliseconds: u32,
        visible_milliseconds: u32,
    ) -> Self {
        let cycle_milliseconds = hidden_milliseconds
            .saturating_add(visible_milliseconds)
            .max(1);
        Self {
            target_node,
            hidden: Duration::from_millis(u64::from(hidden_milliseconds)),
            phase: Timer::new(
                Duration::from_millis(u64::from(cycle_milliseconds)),
                TimerMode::Repeating,
            ),
        }
    }
}

/// Alternates target visibility at the authored button-pulser cadence.
pub(super) fn advance_authored_button_attention_pulse_presentations(
    time: Res<Time>,
    mut pulses: Query<(&UiDocumentOwner, &mut UiAuthoredButtonAttentionPulse)>,
    mut nodes: Query<(&UiNodeId, &UiDocumentOwner, &mut Visibility)>,
) {
    for (owner, mut pulse) in &mut pulses {
        pulse.phase.tick(time.delta());
        let target_is_visible = pulse.phase.elapsed() >= pulse.hidden;
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
