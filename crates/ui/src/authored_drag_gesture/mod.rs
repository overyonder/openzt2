use bevy::{
    ecs::relationship::RelationshipTarget,
    picking::{events::PointerDrag, pointer::PointerButton},
    prelude::*,
    ui::UiGlobalTransform,
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
    layout: Query<(&ComputedNode, &UiGlobalTransform)>,
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
        let geometry = layout.get(receiver).ok().map(|(computed, transform)| {
            let inverse_scale = computed.inverse_scale_factor();
            let extent = computed.size() * inverse_scale;
            let containing = parents
                .get(receiver)
                .ok()
                .and_then(|parent| layout.get(parent.parent()).ok());
            let (position, containing_size) =
                containing.map_or((Vec2::ZERO, None), |(container, parent)| {
                    let local = parent
                        .affine()
                        .inverse()
                        .transform_point2(transform.affine().translation);
                    let container_extent = container.size() * inverse_scale;
                    (
                        local * inverse_scale + (container_extent - extent) * 0.5,
                        Some(container_extent),
                    )
                });
            (extent, position, containing_size)
        });
        let (extent, origin, containing_size) = geometry.unwrap_or_default();
        let containing_size = containing_size.filter(|_| handle.bounded);
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
                    let width = pixel_or(node.width, extent.x);
                    let height = pixel_or(node.height, extent.y);
                    move_axis(
                        &mut node.left,
                        width,
                        delta.x,
                        origin.x,
                        containing_size.map(|size| size.x),
                    );
                    move_axis(
                        &mut node.top,
                        height,
                        delta.y,
                        origin.y,
                        containing_size.map(|size| size.y),
                    );
                }
                UiDragOperation::Resize => {
                    resize_axis(
                        &mut node.width,
                        delta.x,
                        extent.x,
                        handle.minimum_size.x,
                        handle.maximum_size.x,
                    );
                    resize_axis(
                        &mut node.height,
                        delta.y,
                        extent.y,
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

fn pixel_or(value: Val, computed: f32) -> f32 {
    if let Val::Px(value) = value {
        value
    } else {
        computed
    }
}

fn move_axis(offset: &mut Val, size: f32, delta: f32, computed: f32, containing_size: Option<f32>) {
    if delta == 0.0 {
        return;
    }
    let moved = pixel_or(*offset, computed) + delta;
    *offset = px(containing_size.map_or(moved, |limit| moved.clamp(0.0, (limit - size).max(0.0))));
}

fn resize_axis(size: &mut Val, delta: f32, computed: f32, minimum: i32, maximum: i32) {
    if delta == 0.0 {
        return;
    }
    let minimum = minimum.max(0) as f32;
    let maximum = if maximum > 0 {
        (maximum as f32).max(minimum)
    } else {
        f32::INFINITY
    };
    *size = px((pixel_or(*size, computed) + delta).clamp(minimum, maximum));
}
