use bevy::{
    picking::pointer::{PointerId, PointerLocation},
    prelude::*,
    ui::px,
    window::PrimaryWindow,
};
use openzt2_game_data::AssetId;

use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::GameAction;

use super::authored_ui_focus_state::UiFocusScope;
use super::authored_ui_integer_range_components::{UiMaximum, UiMinimum};
use super::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use super::authored_ui_node_projection_components::{UiNodeId, UiValue};
use super::scrollbar_value_and_scroll_position_synchronization::UiScrollDecoration;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct UiSliderPolicy {
    pub(crate) increment: f32,
    pub(crate) minimum_thumb_size: f32,
    pub(crate) axis: UiSliderAxis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiSliderAxis {
    Horizontal,
    Vertical,
    Both,
}

/// Thumb identity and track geometry used to display a slider value.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiSliderPresentation {
    pub thumb: AssetId,
    pub track: Entity,
}

/// Entity-local pointer capture while the primary pointer adjusts a slider.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(super) struct UiSliderPointerActive {
    grab_offset: Vec2,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiSliderValuePresentation {
    pub(super) label: AssetId,
    pub(super) minimum_width: u8,
    pub(super) decimal_places: u8,
    pub(super) percent_suffix: bool,
}

fn ordered_bounds(minimum: i64, maximum: i64) -> (i64, i64) {
    (minimum.min(maximum), minimum.max(maximum))
}

fn slider_step(policy: &UiSliderPolicy) -> f64 {
    let increment = f64::from(policy.increment).abs();
    if increment.is_finite() && increment > f64::EPSILON {
        increment
    } else {
        1.0
    }
}

fn clamp_and_step(value: f64, policy: &UiSliderPolicy, minimum: i64, maximum: i64) -> i64 {
    let (minimum, maximum) = ordered_bounds(minimum, maximum);
    let minimum_f64 = minimum as f64;
    // A range endpoint remains reachable when the span is not a multiple of
    // the authored increment (for example, 528 pixels with a 15-pixel step).
    if value <= minimum_f64 {
        return minimum;
    }
    if value >= maximum as f64 {
        return maximum;
    }
    let step = slider_step(policy);
    let stepped = minimum_f64 + ((value - minimum_f64) / step).round() * step;
    stepped.round().clamp(minimum as f64, maximum as f64) as i64
}

#[cfg(test)]
mod slider_endpoint_tests {
    use super::*;

    #[test]
    fn nonintegral_step_count_preserves_range_endpoints() {
        let policy = UiSliderPolicy {
            increment: 15.0,
            minimum_thumb_size: 8.0,
            axis: UiSliderAxis::Horizontal,
        };
        assert_eq!(clamp_and_step(528.0, &policy, 0, 528), 528);
        assert_eq!(clamp_and_step(0.0, &policy, 0, 528), 0);
        assert_eq!(clamp_and_step(301.0, &policy, 0, 528), 300);
    }

    #[test]
    fn scrollbar_thumb_stays_inside_track_and_top_is_zero() {
        // 100px track, 20px thumb: centre travels from -40 (top) to +40.
        assert_eq!(scroll_ratio(-40.0, 100.0, 20.0), 0.0);
        assert_eq!(scroll_ratio(40.0, 100.0, 20.0), 1.0);
        assert_eq!(scroll_ratio(0.0, 100.0, 20.0), 0.5);
        assert_eq!(scroll_ratio(10.0, 20.0, 20.0), 0.0);
    }
}

pub(super) fn stepped_value(
    value: i64,
    delta: i32,
    policy: &UiSliderPolicy,
    minimum: i64,
    maximum: i64,
) -> i64 {
    clamp_and_step(
        value as f64 + slider_step(policy) * f64::from(delta),
        policy,
        minimum,
        maximum,
    )
}

fn value_ratio(value: i64, minimum: i64, maximum: i64) -> f32 {
    let (minimum, maximum) = ordered_bounds(minimum, maximum);
    let span = maximum - minimum;
    if span == 0 {
        0.0
    } else {
        ((value.clamp(minimum, maximum) - minimum) as f64 / span as f64) as f32
    }
}

fn effective_axis(axis: UiSliderAxis, size: Vec2) -> UiSliderAxis {
    match axis {
        UiSliderAxis::Both if size.x >= size.y => UiSliderAxis::Horizontal,
        UiSliderAxis::Both => UiSliderAxis::Vertical,
        axis => axis,
    }
}

/// Scrollbar ratio for a thumb centre `offset` from the track centre. The
/// authored scrollbar track is the whole region the thumb travels in, so the
/// centre stays half a thumb inside each end, and the value grows down/right.
fn scroll_ratio(offset: f32, track: f32, thumb: f32) -> f32 {
    let travel = track - thumb;
    if travel > 0.0 {
        offset / travel + 0.5
    } else {
        0.0
    }
}

fn value_from_point(
    point: Vec2,
    node: &ComputedNode,
    transform: UiGlobalTransform,
    policy: &UiSliderPolicy,
    scroll_thumb: Option<f32>,
    minimum: i64,
    maximum: i64,
) -> Option<i64> {
    let local = node.normalize_point(transform, point)?;
    let size = node.size();
    let ratio = match (effective_axis(policy.axis, size), scroll_thumb) {
        (UiSliderAxis::Horizontal, None) => local.x + 0.5,
        (UiSliderAxis::Vertical, None) => 0.5 - local.y,
        (UiSliderAxis::Horizontal, Some(thumb)) => scroll_ratio(local.x * size.x, size.x, thumb),
        (UiSliderAxis::Vertical, Some(thumb)) => scroll_ratio(local.y * size.y, size.y, thumb),
        (UiSliderAxis::Both, _) => unreachable!("effective_axis resolves both"),
    }
    .clamp(0.0, 1.0);
    let (minimum, maximum) = ordered_bounds(minimum, maximum);
    let value = minimum as f64 + f64::from(ratio) * (maximum - minimum) as f64;
    Some(clamp_and_step(value, policy, minimum, maximum))
}

/// Applies authored bounds immediately and suppresses redundant value writes.
pub(super) fn apply_slider_bounds(
    mut sliders: Query<
        (&UiSliderPolicy, &UiMinimum, &UiMaximum, &mut UiValue, Option<&UiScrollDecoration>),
        Or<(Changed<UiMinimum>, Changed<UiMaximum>)>,
    >,
) {
    for (policy, minimum, maximum, mut value, scroll) in &mut sliders {
        let (lower, upper) = ordered_bounds(minimum.0, maximum.0);
        let next = if scroll.is_some() {
            value.0.clamp(lower, upper)
        } else {
            clamp_and_step(value.0 as f64, policy, lower, upper)
        };
        value.set_if_neq(UiValue(next));
    }
}

/// Adjusts the focused slider through the same stepped, clamped value path as
/// pointer interaction.
pub(super) fn adjust_focused_sliders(
    mut actions: MessageReader<ActionRequest>,
    scopes: Query<(Entity, &UiFocusScope)>,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    mut sliders: Query<(&UiSliderPolicy, &UiMinimum, &UiMaximum, &mut UiValue, Option<&UiScrollDecoration>)>,
) {
    let active_scope = context.active_scope();
    let modal = context.top_modal();
    for request in actions.read() {
        for (root, scope) in &scopes {
            if Some(root) != active_scope {
                continue;
            }
            let Some(focused) = scope.focused else {
                continue;
            };
            if !context.eligible_in_scope(focused, root, modal) {
                continue;
            }
            let Ok((policy, minimum, maximum, mut value, scroll)) = sliders.get_mut(focused) else {
                continue;
            };
            let axis = scroll.map_or(policy.axis, |scroll| scroll.axis);
            let direction = match (axis, request.action) {
                (UiSliderAxis::Horizontal | UiSliderAxis::Both, GameAction::NavigateLeft)
                | (UiSliderAxis::Vertical | UiSliderAxis::Both, GameAction::NavigateDown) => -1.0,
                (UiSliderAxis::Horizontal | UiSliderAxis::Both, GameAction::NavigateRight)
                | (UiSliderAxis::Vertical | UiSliderAxis::Both, GameAction::NavigateUp) => 1.0,
                _ => continue,
            };
            let direction = if scroll.is_some()
                && matches!(request.action, GameAction::NavigateUp | GameAction::NavigateDown)
            { -direction } else { direction };
            let next = value.0 as f64 + slider_step(policy) * direction;
            let next = clamp_and_step(next, policy, minimum.0, maximum.0);
            value.set_if_neq(UiValue(next));
        }
    }
}

/// Samples Bevy Picking's active mouse pointer. Pressing the authored
/// thumb-centre interval or the visible thumb captures the slider entity so
/// dragging continues after the pointer leaves either node.
pub(super) fn adjust_sliders_from_pointer(
    mut commands: Commands,
    primary_pointer: Res<openzt2_interface_state::input::input_types::PrimaryPointerInputState>,
    pointers: Query<(&PointerId, &PointerLocation)>,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    capture: Res<openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture>,
    mut sliders: Query<(
        Entity,
        Option<&UiInteractionEnabled>,
        &UiSliderPresentation,
        &UiSliderPolicy,
        &UiMinimum,
        &UiMaximum,
        &mut UiValue,
        Option<&UiSliderPointerActive>,
        Option<&UiScrollDecoration>,
    )>,
    tracks: Query<(&ComputedNode, &UiGlobalTransform)>,
    thumbs: Query<(Entity, &UiNodeId, &ComputedNode, &UiGlobalTransform)>,
    parents: Query<&ChildOf>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let modal = context.top_modal();
    // Pointer positions are logical; UI node transforms are physical.
    let scale = windows.single().map_or(1.0, Window::scale_factor);
    let point = pointers.iter().find_map(|(id, location)| {
        if *id == PointerId::Mouse {
            location.location().map(|location| location.position * scale)
        } else {
            None
        }
    });
    for (entity, enabled, presentation, policy, minimum, maximum, mut value, active, scroll) in
        &mut sliders
    {
        // Pointer capture already picked this slider; keyboard focus scope
        // (the topmost focusable document) must not gate it, or a HUD slider
        // such as the catalogue scrollbar never drags.
        let eligible = enabled.is_none_or(|enabled| enabled.0)
            && context.visible_and_enabled(entity)
            && modal.is_none_or(|modal| context.is_in_modal(entity, modal));
        if !eligible || (!primary_pointer.pressed && !primary_pointer.just_released) {
            if active.is_some() {
                commands.entity(entity).remove::<UiSliderPointerActive>();
            }
            continue;
        }
        let enabled = eligible;
        let track = tracks.get(presentation.track).ok();
        if primary_pointer.just_pressed
            && point.is_some_and(|point| {
                tracks
                    .get(entity)
                    .is_ok_and(|(node, transform)| node.contains_point(*transform, point))
            })
        {
            debug!(?entity, ?point, enabled, minimum = minimum.0, maximum = maximum.0,
                value = value.0, ?policy,
                track_size = ?track.map(|(node, _)| node.size()),
                track_transform = ?track.map(|(_, transform)| transform),
                "pressed slider runtime bounds");
        }
        let pointer_over_interval = point.is_some_and(|point| {
            track.is_some_and(|(node, transform)| node.contains_point(*transform, point))
        });
        let thumb = thumbs.iter().find(|(candidate, id, _, _)| {
            id.id == presentation.thumb && node_is_descendant_of(*candidate, entity, &parents)
        });
        let pointer_over_thumb = point.is_some_and(|point| {
            thumb.is_some_and(|(_, _, node, transform)| node.contains_point(*transform, point))
        });
        let starting = primary_pointer.just_pressed
            && capture.target.is_some_and(|target| {
                target == entity || node_is_descendant_of(target, entity, &parents)
            })
            && (pointer_over_interval || pointer_over_thumb);
        if !starting && active.is_none() {
            continue;
        }
        let grab_offset = active.map_or_else(
            || {
                if starting && pointer_over_thumb {
                    point.zip(thumb).map_or(Vec2::ZERO, |(point, (_, _, _, transform))| {
                        point - transform.affine().translation
                    })
                } else {
                    Vec2::ZERO
                }
            },
            |active| active.grab_offset,
        );
        if let Some(next) = track.and_then(|(node, transform)| {
            let scroll_thumb = scroll.map(|_| {
                let size = thumb.map_or(Vec2::ZERO, |(_, _, thumb, _)| thumb.size());
                match effective_axis(scroll.map_or(policy.axis, |scroll| scroll.axis), node.size()) {
                    UiSliderAxis::Horizontal => size.x,
                    _ => size.y,
                }
            });
            point.and_then(|point| {
                value_from_point(
                    point - grab_offset,
                    node,
                    *transform,
                    &UiSliderPolicy {
                        axis: scroll.map_or(policy.axis, |scroll| scroll.axis),
                        ..*policy
                    },
                    scroll_thumb,
                    minimum.0,
                    maximum.0,
                )
            })
        }) {
            value.set_if_neq(UiValue(next));
        }
        if starting {
            commands.entity(entity).insert(UiSliderPointerActive { grab_offset });
        }
        if primary_pointer.just_released {
            commands.entity(entity).remove::<UiSliderPointerActive>();
        }
    }
}

fn node_is_descendant_of(node: Entity, ancestor: Entity, parents: &Query<&ChildOf>) -> bool {
    let mut node = node;
    while let Ok(parent) = parents.get(node) {
        node = parent.parent();
        if node == ancestor {
            return true;
        }
    }
    false
}

/// Sizes scrollbar thumbs without changing the authored track geometry.
pub(super) fn size_scrollbar_thumbs(
    sliders: Query<(Entity, &UiSliderPresentation, &UiSliderPolicy, &UiScrollDecoration)>,
    bounds: Query<&ComputedNode>,
    children: Query<&Children>,
    mut thumbs: Query<(&UiNodeId, &mut Node)>,
) {
    for (slider, presentation, policy, scroll) in &sliders {
        let (Ok(track), Ok(owner)) = (bounds.get(presentation.track), bounds.get(scroll.owner)) else {
            continue;
        };
        let Some(thumb) = children.iter_descendants(slider).find(|entity| {
            thumbs.get(*entity).is_ok_and(|(id, _)| id.id == presentation.thumb)
        }) else {
            continue;
        };
        let Ok((_, mut node)) = thumbs.get_mut(thumb) else {
            continue;
        };
        let axis = effective_axis(scroll.axis, track.size());
        let (track_length, viewport, content) = match axis {
            UiSliderAxis::Horizontal => (track.size().x, owner.size().x, owner.content_size().x),
            UiSliderAxis::Vertical => (track.size().y, owner.size().y, owner.content_size().y),
            UiSliderAxis::Both => unreachable!("effective_axis resolves both"),
        };
        let length = track_length * track.inverse_scale_factor();
        if length <= 0.0 {
            continue;
        }
        let proportion = if content > viewport { viewport / content } else { 1.0 };
        let minimum = policy.minimum_thumb_size.max(0.0).min(length);
        let size = px((length * proportion).clamp(minimum, length));
        match axis {
            UiSliderAxis::Horizontal => {
                if node.width != size { node.width = size; }
            }
            UiSliderAxis::Vertical => {
                if node.height != size { node.height = size; }
            }
            UiSliderAxis::Both => unreachable!("effective_axis resolves both"),
        }
    }
}

/// Projects progress onto this frame's layout before clipping and pixel snapping.
pub(super) fn present_slider_values(
    sliders: Query<(
        Entity, &UiSliderPresentation, &UiSliderPolicy,
        &UiMinimum, &UiMaximum, &UiValue, Option<&UiScrollDecoration>,
    )>,
    children: Query<&Children>,
    scroll_owners: Query<(&ComputedNode, &ScrollPosition)>,
    mut nodes: ParamSet<(
        Query<(&ComputedNode, &UiGlobalTransform, Option<&UiNodeId>)>,
        Query<&mut UiGlobalTransform>,
    )>,
) {
    for (slider, presentation, policy, minimum, maximum, value, scroll) in &sliders {
        if presentation.thumb == AssetId::default() { continue; }
        let geometry = nodes.p0();
        let Ok((track, track_transform, _)) = geometry.get(presentation.track) else {
            continue;
        };
        let Some(thumb) = children.iter_descendants(slider).find(|entity| {
            geometry.get(*entity).is_ok_and(|(_, _, id)| {
                id.is_some_and(|id| id.id == presentation.thumb)
            })
        }) else {
            continue;
        };
        let Ok((thumb_node, thumb_transform, _)) = geometry.get(thumb) else {
            continue;
        };
        let axis = effective_axis(scroll.map_or(policy.axis, |scroll| scroll.axis), track.size());
        let ratio = if let Some(scroll) = scroll {
            let Ok((owner, position)) = scroll_owners.get(scroll.owner) else { continue; };
            let overflow = (owner.content_size() - owner.size()).max(Vec2::ZERO)
                * owner.inverse_scale_factor();
            let (offset, limit) = match axis {
                UiSliderAxis::Horizontal => (position.x, overflow.x),
                UiSliderAxis::Vertical => (position.y, overflow.y),
                UiSliderAxis::Both => unreachable!("effective_axis resolves both"),
            };
            if limit > 0.0 { (offset / limit).clamp(0.0, 1.0) } else { 0.0 }
        } else {
            value_ratio(value.0, minimum.0, maximum.0)
        };
        let (track_length, thumb_length) = match axis {
            UiSliderAxis::Horizontal => (track.size().x, thumb_node.size().x),
            UiSliderAxis::Vertical => (track.size().y, thumb_node.size().y),
            UiSliderAxis::Both => unreachable!("effective_axis resolves both"),
        };
        let travel = if scroll.is_some() { (track_length - thumb_length).max(0.0) } else { track_length };
        let offset = if axis == UiSliderAxis::Vertical && scroll.is_none() {
            (0.5 - ratio) * travel
        } else {
            (ratio - 0.5) * travel
        };
        let Some(inverse_track) = track_transform.try_inverse() else {
            continue;
        };
        let current = thumb_transform.affine().translation;
        let mut point = inverse_track.transform_point2(current);
        match axis {
            UiSliderAxis::Horizontal => point.x = offset,
            UiSliderAxis::Vertical => point.y = offset,
            UiSliderAxis::Both => unreachable!("effective_axis resolves both"),
        }
        let delta = track_transform.affine().transform_point2(point) - current;
        // Layout reconstructs these transforms each frame. Local translations
        // remain authored, so rounded render coordinates never feed back into them.
        let mut transforms = nodes.p1();
        for entity in std::iter::once(thumb).chain(children.iter_descendants(thumb)) {
            let Ok(mut transform) = transforms.get_mut(entity) else { continue; };
            let mut next = transform.affine();
            next.translation += delta;
            let next = UiGlobalTransform::from(next);
            if *transform != next { *transform = next; }
        }
    }
}

/// Formats the slider value using the authored width, precision and percent suffix.
pub(super) fn present_slider_value_labels(
    sliders: Query<(Entity, &UiSliderValuePresentation, &UiValue)>,
    children: Query<&Children>,
    mut labels: Query<(Entity, &UiNodeId, &mut Text)>,
) {
    for (slider, presentation, value) in &sliders {
        // Search only this slider's own subtree for its label.
        let Some(label) = children.iter_descendants(slider).find(|candidate| {
            labels
                .get(*candidate)
                .is_ok_and(|(_, id, _)| id.id == presentation.label)
        }) else {
            continue;
        };
        let Ok((_, _, mut text)) = labels.get_mut(label) else {
            continue;
        };
        let number = if presentation.decimal_places == 0 {
            format!(
                "{:width$}",
                value.0,
                width = usize::from(presentation.minimum_width)
            )
        } else {
            format!(
                "{:width$.precision$}",
                value.0 as f64,
                width = usize::from(presentation.minimum_width),
                precision = usize::from(presentation.decimal_places)
            )
        };
        let next = if presentation.percent_suffix {
            format!("{number}%")
        } else {
            number
        };
        if text.0 != next {
            text.0 = next;
        }
    }
}
