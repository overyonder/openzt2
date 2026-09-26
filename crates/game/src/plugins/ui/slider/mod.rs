use bevy::{
    picking::pointer::{PointerId, PointerLocation},
    prelude::*,
    ui::px,
};
use openzt2_game_data::AssetId;

use crate::plugins::input::input_types::{ActionRequest, GameAction};

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
pub(crate) enum UiSliderAxis {
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
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiSliderPointerActive;

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

fn value_from_point(
    point: Vec2,
    node: &ComputedNode,
    transform: UiGlobalTransform,
    policy: &UiSliderPolicy,
    minimum: i64,
    maximum: i64,
) -> Option<i64> {
    let local = node.normalize_point(transform, point)?;
    let ratio = match effective_axis(policy.axis, node.size()) {
        UiSliderAxis::Horizontal => local.x + 0.5,
        UiSliderAxis::Vertical => 0.5 - local.y,
        UiSliderAxis::Both => unreachable!("effective_axis resolves both"),
    }
    .clamp(0.0, 1.0);
    let (minimum, maximum) = ordered_bounds(minimum, maximum);
    let value = minimum as f64 + f64::from(ratio) * (maximum - minimum) as f64;
    Some(clamp_and_step(value, policy, minimum, maximum))
}

/// Applies authored bounds immediately and suppresses redundant value writes.
pub(super) fn apply_slider_bounds(
    mut sliders: Query<
        (&UiSliderPolicy, &UiMinimum, &UiMaximum, &mut UiValue),
        Or<(Changed<UiMinimum>, Changed<UiMaximum>)>,
    >,
) {
    for (policy, minimum, maximum, mut value) in &mut sliders {
        let next = clamp_and_step(value.0 as f64, policy, minimum.0, maximum.0);
        value.set_if_neq(UiValue(next));
    }
}

/// Adjusts the focused slider through the same stepped, clamped value path as
/// pointer interaction.
pub(super) fn adjust_focused_sliders(
    mut actions: MessageReader<ActionRequest>,
    scopes: Query<(Entity, &UiFocusScope)>,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    mut sliders: Query<(&UiSliderPolicy, &UiMinimum, &UiMaximum, &mut UiValue)>,
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
            let Ok((policy, minimum, maximum, mut value)) = sliders.get_mut(focused) else {
                continue;
            };
            let direction = match (policy.axis, request.action) {
                (UiSliderAxis::Horizontal | UiSliderAxis::Both, GameAction::NavigateLeft)
                | (UiSliderAxis::Vertical | UiSliderAxis::Both, GameAction::NavigateDown) => -1.0,
                (UiSliderAxis::Horizontal | UiSliderAxis::Both, GameAction::NavigateRight)
                | (UiSliderAxis::Vertical | UiSliderAxis::Both, GameAction::NavigateUp) => 1.0,
                _ => continue,
            };
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
    primary_pointer: Res<crate::plugins::input::input_types::PrimaryPointerInputState>,
    pointers: Query<(&PointerId, &PointerLocation)>,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    capture: Res<super::picking::UiPointerCapture>,
    mut sliders: Query<(
        Entity,
        Option<&UiInteractionEnabled>,
        &UiSliderPresentation,
        &UiSliderPolicy,
        &UiMinimum,
        &UiMaximum,
        &mut UiValue,
        Option<&UiSliderPointerActive>,
    )>,
    tracks: Query<(&ComputedNode, &UiGlobalTransform)>,
    thumbs: Query<(Entity, &UiNodeId, &ComputedNode, &UiGlobalTransform)>,
    parents: Query<&ChildOf>,
) {
    let active_scope = context.active_scope();
    let modal = context.top_modal();
    let point = pointers.iter().find_map(|(id, location)| {
        if *id == PointerId::Mouse {
            location.location().map(|location| location.position)
        } else {
            None
        }
    });
    for (entity, enabled, presentation, policy, minimum, maximum, mut value, active) in &mut sliders
    {
        let eligible = enabled.is_none_or(|enabled| enabled.0)
            && active_scope.is_some_and(|scope| context.eligible_in_scope(entity, scope, modal));
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
        let pointer_over_thumb = point.is_some_and(|point| {
            thumbs.iter().any(|(candidate, id, node, transform)| {
                id.id == presentation.thumb
                    && node.contains_point(*transform, point)
                    && node_is_descendant_of(candidate, entity, &parents)
            })
        });
        let starting = primary_pointer.just_pressed
            && capture.target.is_some_and(|target| {
                target == entity || node_is_descendant_of(target, entity, &parents)
            })
            && (pointer_over_interval || pointer_over_thumb);
        if !starting && active.is_none() {
            continue;
        }
        if let Some(next) = track.and_then(|(node, transform)| {
            point.and_then(|point| {
                value_from_point(point, node, *transform, policy, minimum.0, maximum.0)
            })
        }) {
            value.set_if_neq(UiValue(next));
        }
        if starting {
            commands.entity(entity).insert(UiSliderPointerActive);
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

/// Positions the thumb along the track for the current value.
pub(super) fn present_slider_values(
    sliders: Query<(
        Entity,
        &UiSliderPresentation,
        &UiSliderPolicy,
        &UiMinimum,
        &UiMaximum,
        &UiValue,
        Option<&UiScrollDecoration>,
    )>,
    tracks: Query<(&ComputedNode, &UiGlobalTransform)>,
    scroll_owners: Query<&ComputedNode>,
    parents: Query<&ChildOf>,
    mut thumbs: Query<(
        Entity,
        &UiNodeId,
        &UiGlobalTransform,
        &mut Node,
        &mut UiTransform,
    )>,
) {
    for (slider, presentation, policy, minimum, maximum, value, scroll) in &sliders {
        if presentation.thumb == AssetId::default() {
            continue;
        }
        let Ok((track, track_transform)) = tracks.get(presentation.track) else {
            continue;
        };
        let ratio = value_ratio(value.0, minimum.0, maximum.0);
        let axis = effective_axis(policy.axis, track.size());
        let Some((_, _, thumb_transform, mut thumb_node, mut transform)) =
            thumbs.iter_mut().find(|(candidate, id, _, _, _)| {
                if id.id != presentation.thumb {
                    return false;
                }
                let mut entity = *candidate;
                while let Ok(parent) = parents.get(entity) {
                    entity = parent.parent();
                    if entity == slider {
                        return true;
                    }
                }
                false
            })
        else {
            continue;
        };
        let inverse_scale = track.inverse_scale_factor();
        if let Some(scroll) = scroll {
            let Ok(scroll_owner) = scroll_owners.get(scroll.owner) else {
                continue;
            };
            let viewport = scroll_owner.size();
            let content = scroll_owner.content_size().max(viewport);
            let authored_track = track.size() * inverse_scale;
            let proportion = Vec2::new(
                if content.x > 0.0 {
                    viewport.x / content.x
                } else {
                    1.0
                },
                if content.y > 0.0 {
                    viewport.y / content.y
                } else {
                    1.0
                },
            );
            match effective_axis(policy.axis, track.size()) {
                UiSliderAxis::Horizontal => {
                    let minimum = policy.minimum_thumb_size.min(authored_track.x);
                    let width = (authored_track.x * proportion.x).clamp(minimum, authored_track.x);
                    if thumb_node.width != px(width) {
                        thumb_node.width = px(width);
                    }
                }
                UiSliderAxis::Vertical => {
                    let minimum = policy.minimum_thumb_size.min(authored_track.y);
                    let height = (authored_track.y * proportion.y).clamp(minimum, authored_track.y);
                    if thumb_node.height != px(height) {
                        thumb_node.height = px(height);
                    }
                }
                UiSliderAxis::Both => unreachable!("effective_axis resolves both"),
            }
        }
        let authored_interval_point = match axis {
            UiSliderAxis::Horizontal => Vec2::new((ratio - 0.5) * track.size().x, 0.0),
            UiSliderAxis::Vertical => Vec2::new(0.0, (0.5 - ratio) * track.size().y),
            UiSliderAxis::Both => unreachable!("effective_axis resolves both"),
        };
        let desired_thumb_centre = track_transform
            .affine()
            .transform_point2(authored_interval_point);
        let current_thumb_centre = thumb_transform.affine().transform_point2(Vec2::ZERO);
        let correction = track_transform.affine().matrix2.inverse()
            * (desired_thumb_centre - current_thumb_centre);
        let translation = match axis {
            UiSliderAxis::Horizontal => Val2::new(
                px(translation_pixels(transform.translation.x) + correction.x),
                transform.translation.y,
            ),
            UiSliderAxis::Vertical => Val2::new(
                transform.translation.x,
                px(translation_pixels(transform.translation.y) + correction.y),
            ),
            UiSliderAxis::Both => unreachable!("effective_axis resolves both"),
        };
        if transform.translation != translation {
            transform.translation = translation;
        }
    }
}

fn translation_pixels(translation: Val) -> f32 {
    match translation {
        Val::Px(pixels) => pixels,
        _ => 0.0,
    }
}

/// Formats the slider value using the authored width, precision and percent suffix.
pub(super) fn present_slider_value_labels(
    sliders: Query<(Entity, &UiSliderValuePresentation, &UiValue)>,
    parents: Query<&ChildOf>,
    mut labels: Query<(Entity, &UiNodeId, &mut Text)>,
) {
    for (slider, presentation, value) in &sliders {
        let Some((_, _, mut text)) = labels.iter_mut().find(|(candidate, id, _)| {
            if id.id != presentation.label {
                return false;
            }
            let mut entity = *candidate;
            while let Ok(parent) = parents.get(entity) {
                entity = parent.parent();
                if entity == slider {
                    return true;
                }
            }
            false
        }) else {
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
