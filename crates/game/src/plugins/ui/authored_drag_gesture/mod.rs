use bevy::{input::mouse::AccumulatedMouseMotion, prelude::*};
use openzt2_game_data::ui_document::widget_control::UiDragOperation;

use super::slider::UiSliderAxis;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiAuthoredDragGesturePolicy {
    operation: UiDragOperation,
    axis: UiSliderAxis,
    flip_axis: bool,
}

impl UiAuthoredDragGesturePolicy {
    pub(super) const fn from_authored_gesture(
        operation: UiDragOperation,
        axis: UiSliderAxis,
        flip_axis: bool,
    ) -> Self {
        Self {
            operation,
            axis,
            flip_axis,
        }
    }
}

/// Applies the three authored drag gestures directly to Bevy's layout or
/// scroll component. Source lowering already erased the original message
/// indirection; this is ordinary pointer-driven UI behavior.
pub(super) fn apply_authored_drag_gestures_to_bevy_layout_or_scroll_position(
    motion: Res<AccumulatedMouseMotion>,
    mut gestures: Query<(
        &UiAuthoredDragGesturePolicy,
        &Interaction,
        &mut Node,
        Option<&mut ScrollPosition>,
    )>,
) {
    if motion.delta == Vec2::ZERO {
        return;
    }
    for (policy, interaction, mut node, scroll) in &mut gestures {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let mut delta = motion.delta;
        if policy.flip_axis {
            delta = Vec2::new(delta.y, delta.x);
        }
        match policy.axis {
            UiSliderAxis::Horizontal => delta.y = 0.0,
            UiSliderAxis::Vertical => delta.x = 0.0,
            UiSliderAxis::Both => {}
        }
        match policy.operation {
            UiDragOperation::Move => {
                if let Val::Px(left) = &mut node.left {
                    *left += delta.x;
                }
                if let Val::Px(top) = &mut node.top {
                    *top += delta.y;
                }
            }
            UiDragOperation::Resize => {
                if let Val::Px(width) = &mut node.width {
                    *width = (*width + delta.x).max(0.0);
                }
                if let Val::Px(height) = &mut node.height {
                    *height = (*height + delta.y).max(0.0);
                }
            }
            UiDragOperation::Scroll => {
                if let Some(mut position) = scroll {
                    position.x = (position.x - delta.x).max(0.0);
                    position.y = (position.y - delta.y).max(0.0);
                }
            }
        }
    }
}
