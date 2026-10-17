use bevy::{
    picking::pointer::{PointerId, PointerLocation},
    prelude::*,
    ui::px,
    window::PrimaryWindow,
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
        if let Some(next) = track.and_then(|(node, transform)| {
            let scroll_thumb = scroll.map(|_| {
                let size = thumb.map_or(Vec2::ZERO, |(_, _, thumb, _)| thumb.size());
                match effective_axis(policy.axis, node.size()) {
                    UiSliderAxis::Horizontal => size.x,
                    _ => size.y,
                }
            });
            point.and_then(|point| {
                value_from_point(
                    point,
                    node,
                    *transform,
                    policy,
                    scroll_thumb,
                    minimum.0,
                    maximum.0,
                )
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
    children: Query<&Children>,
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
        // Search only this slider's own subtree for its thumb.
        let Some(thumb) = children.iter_descendants(slider).find(|candidate| {
            thumbs
                .get(*candidate)
                .is_ok_and(|(_, id, _, _, _)| id.id == presentation.thumb)
        }) else {
            continue;
        };
        let Ok((_, _, thumb_transform, mut thumb_node, mut transform)) = thumbs.get_mut(thumb)
        else {
            continue;
        };
        let inverse_scale = track.inverse_scale_factor();
        // Physical thumb length along the axis, only for scrollbars.
        let mut scroll_thumb = None;
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
                    scroll_thumb = Some(width / inverse_scale);
                }
                UiSliderAxis::Vertical => {
                    let minimum = policy.minimum_thumb_size.min(authored_track.y);
                    let height = (authored_track.y * proportion.y).clamp(minimum, authored_track.y);
                    if thumb_node.height != px(height) {
                        thumb_node.height = px(height);
                    }
                    scroll_thumb = Some(height / inverse_scale);
                }
                UiSliderAxis::Both => unreachable!("effective_axis resolves both"),
            }
        }
        let authored_interval_point = match (axis, scroll_thumb) {
            (UiSliderAxis::Horizontal, None) => Vec2::new((ratio - 0.5) * track.size().x, 0.0),
            (UiSliderAxis::Vertical, None) => Vec2::new(0.0, (0.5 - ratio) * track.size().y),
            (UiSliderAxis::Horizontal, Some(thumb)) => {
                Vec2::new((ratio - 0.5) * (track.size().x - thumb).max(0.0), 0.0)
            }
            (UiSliderAxis::Vertical, Some(thumb)) => {
                Vec2::new(0.0, (ratio - 0.5) * (track.size().y - thumb).max(0.0))
            }
            (UiSliderAxis::Both, _) => unreachable!("effective_axis resolves both"),
        };
        let desired_thumb_centre = track_transform
            .affine()
            .transform_point2(authored_interval_point);
        let current_thumb_centre = thumb_transform.affine().transform_point2(Vec2::ZERO);
        // `UiTransform` translation is in logical pixels but the transforms
        // are physical; without `inverse_scale` HiDPI overshoots every frame.
        let correction = track_transform.affine().matrix2.inverse()
            * (desired_thumb_centre - current_thumb_centre)
            * inverse_scale;
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
