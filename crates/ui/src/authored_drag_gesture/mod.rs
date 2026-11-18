use bevy::{
    ecs::relationship::RelationshipTarget,
    picking::{events::PointerDrag, pointer::PointerButton},
    prelude::*,
};
use openzt2_game_data::ui_document::widget_control::UiDragOperation;

use super::slider::UiSliderAxis;

#[derive(Component, Default)]
pub(super) struct UiAuthoredDragHandle {
    pub(super) minimum_size: IVec2,
    pub(super) maximum_size: IVec2,
    pub(super) bounded: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiAuthoredDragCommand {
    pub(super) operation: UiDragOperation,
    pub(super) axis: UiSliderAxis,
    pub(super) flip_axis: bool,
}

pub(super) fn apply_authored_drag_gestures_to_bevy_layout_or_scroll_position(
    mut drags: MessageReader<PointerDrag>,
    handles: Query<(&UiAuthoredDragHandle, &ChildOf, &Children)>,
    drag_commands: Query<&UiAuthoredDragCommand>,
    parents: Query<&ChildOf>,
    layout: Query<&ComputedNode>,
    mut receivers: Query<(&mut Node, Option<&mut ScrollPosition>)>,
    ui_scale: Res<UiScale>,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    modal: Res<openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture>,
) {
    for drag in drags.read() {
        if !drag.pointer.id.is_mouse() || drag.button != PointerButton::Primary {
            continue;
        }
        let enclosing_handle = std::iter::once(drag.entity)
            .chain(parents.iter_ancestors::<ChildOf>(drag.entity))
            .find_map(|entity| handles.get(entity).ok().map(|handle| (entity, handle)));
        let default_handle = UiAuthoredDragHandle::default();
        let (handle_entity, receiver, handle, commands, standalone) = match enclosing_handle {
            Some((entity, (handle, parent, children))) => {
                (entity, parent.parent(), handle, Some(children), None)
            }
            None if drag_commands.contains(drag.entity) => (
                drag.entity,
                drag.entity,
                &default_handle,
                None,
                Some(drag.entity),
            ),
            None => continue,
        };
        if !context.visible_and_enabled(handle_entity)
            || modal
                .0
                .is_some_and(|modal| !context.is_in_modal(handle_entity, modal))
        {
            continue;
        }
        let Ok((mut node, mut scroll)) = receivers.get_mut(receiver) else {
            continue;
        };
        let containing_size = handle
            .bounded
            .then(|| {
                let container = parents.get(receiver).ok()?;
                let layout = layout.get(container.parent()).ok()?;
                Some(layout.size() * layout.inverse_scale_factor())
            })
            .flatten();
        for command in commands
            .into_iter()
            .flat_map(|children| children.iter())
            .chain(standalone)
            .filter_map(|entity| drag_commands.get(entity).ok())
        {
            let mut delta = drag.delta / ui_scale.0;
            if command.flip_axis {
                delta = delta.yx();
            }
            match command.axis {
                UiSliderAxis::Horizontal => delta.y = 0.0,
                UiSliderAxis::Vertical => delta.x = 0.0,
                UiSliderAxis::Both => {}
            }
            match command.operation {
                UiDragOperation::Move => {
                    let width = node.width;
                    let height = node.height;
                    move_axis(
                        &mut node.left,
                        width,
                        delta.x,
                        containing_size.map(|size| size.x),
                    );
                    move_axis(
                        &mut node.top,
                        height,
                        delta.y,
                        containing_size.map(|size| size.y),
                    );
                }
                UiDragOperation::Resize => {
                    resize_axis(
                        &mut node.width,
                        delta.x,
                        handle.minimum_size.x,
                        handle.maximum_size.x,
                    );
                    resize_axis(
                        &mut node.height,
                        delta.y,
                        handle.minimum_size.y,
                        handle.maximum_size.y,
                    );
                }
                UiDragOperation::Scroll => {
                    if let Some(position) = scroll.as_deref_mut() {
                        position.x = (position.x - delta.x).max(0.0);
                        position.y = (position.y - delta.y).max(0.0);
                    }
                }
            }
        }
    }
}

fn move_axis(offset: &mut Val, size: Val, delta: f32, containing_size: Option<f32>) {
    if delta == 0.0 {
        return;
    }
    if let Val::Px(offset) = offset {
        *offset += delta;
        if let (Some(limit), Val::Px(size)) = (containing_size, size) {
            *offset = offset.clamp(0.0, (limit - size).max(0.0));
        }
    }
}

fn resize_axis(size: &mut Val, delta: f32, minimum: i32, maximum: i32) {
    if delta == 0.0 {
        return;
    }
    if let Val::Px(size) = size {
        let minimum = minimum.max(0) as f32;
        let maximum = if maximum > 0 {
            (maximum as f32).max(minimum)
        } else {
            f32::INFINITY
        };
        *size = (*size + delta).clamp(minimum, maximum);
    }
}
