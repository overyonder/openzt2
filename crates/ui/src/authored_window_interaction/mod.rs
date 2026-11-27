use bevy::{
    picking::{events::PointerDrag, pointer::PointerButton},
    prelude::*,
};

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

pub(super) fn move_authored_windows_from_pointer_drags(
    mut drags: MessageReader<PointerDrag>,
    mut windows: ParamSet<(
        (
            super::active_authored_ui_context::ActiveAuthoredUiContext,
            Query<&UiAuthoredWindowInteractionPolicy, With<Node>>,
        ),
        Query<&mut Node, With<UiAuthoredWindowInteractionPolicy>>,
    )>,
    modal: Res<openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture>,
) {
    for drag in drags.read() {
        if !drag.pointer.id.is_mouse() || drag.button != PointerButton::Primary {
            continue;
        }
        let (context, policies) = windows.p0();
        if !policies
            .get(drag.entity)
            .is_ok_and(|policy| policy.draggable)
            || !context.visible_and_enabled(drag.entity)
            || modal
                .0
                .is_some_and(|modal| !context.is_in_modal(drag.entity, modal))
        {
            continue;
        }
        let mut nodes = windows.p1();
        let Ok(mut node) = nodes.get_mut(drag.entity) else {
            continue;
        };
        if let Val::Px(left) = &mut node.left {
            *left += drag.delta.x;
        }
        if let Val::Px(top) = &mut node.top {
            *top += drag.delta.y;
        }
    }
}
