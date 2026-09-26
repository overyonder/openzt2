use bevy::{
    input::mouse::{MouseScrollUnit, MouseWheel},
    prelude::*,
};

use super::authored_reusable_list_and_table_runtime_types::UiListPolicy;
use super::authored_window_interaction::UiAuthoredWindowInteractionPolicy;

pub(super) fn scroll_hovered_authored_lists_or_wheel_scroll_windows_from_mouse_wheel(
    mut wheel: MessageReader<MouseWheel>,
    mut panels: Query<(
        &mut ScrollPosition,
        &ComputedNode,
        &Interaction,
        Option<&UiListPolicy>,
        Option<&UiAuthoredWindowInteractionPolicy>,
    )>,
) {
    let delta = wheel.read().fold(0.0, |total, event| {
        total
            - event.y
                * if event.unit == MouseScrollUnit::Line {
                    28.0
                } else {
                    1.0
                }
    });
    if delta == 0.0 {
        return;
    }
    for (mut position, computed, interaction, list, window) in &mut panels {
        let scrollable =
            list.is_some() || window.is_some_and(|policy| policy.accepts_wheel_scroll());
        if !scrollable || *interaction != Interaction::Hovered {
            continue;
        }
        let maximum = (computed.content_size().y - computed.size().y).max(0.0)
            * computed.inverse_scale_factor();
        position.y = (position.y + delta).clamp(0.0, maximum);
    }
}
