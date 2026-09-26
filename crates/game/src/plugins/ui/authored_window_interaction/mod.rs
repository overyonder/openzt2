use bevy::{input::mouse::AccumulatedMouseMotion, prelude::*};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiAuthoredWindowInteractionPolicy {
    draggable: bool,
    wheel_scroll: bool,
}

impl UiAuthoredWindowInteractionPolicy {
    pub(super) const fn from_authored_interaction(draggable: bool, wheel_scroll: bool) -> Self {
        Self {
            draggable,
            wheel_scroll,
        }
    }

    pub(super) const fn accepts_wheel_scroll(self) -> bool {
        self.wheel_scroll
    }
}

pub(super) fn drag_authored_windows_from_accumulated_pointer_motion(
    motion: Res<AccumulatedMouseMotion>,
    mut windows: Query<(&UiAuthoredWindowInteractionPolicy, &Interaction, &mut Node)>,
) {
    if motion.delta == Vec2::ZERO {
        return;
    }
    for (policy, interaction, mut node) in &mut windows {
        if policy.draggable && *interaction == Interaction::Pressed {
            if let Val::Px(left) = &mut node.left {
                *left += motion.delta.x;
            }
            if let Val::Px(top) = &mut node.top {
                *top += motion.delta.y;
            }
        }
    }
}
