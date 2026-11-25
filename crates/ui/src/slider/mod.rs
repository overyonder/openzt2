use bevy::{
    picking::{
        events::{PointerDrag, PointerPress},
        hover::PointerCaptureMap,
        pointer::{PointerButton, PointerId},
    },
    prelude::*,
    ui::px,
    ui_widgets::{Slider, SliderRange, SliderStep, SliderValue, ValueChange},
    window::PrimaryWindow,
};
use openzt2_game_data::AssetId;

use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::GameAction;

use super::authored_scrollbar::ScrollsViewport;
use super::authored_ui_integer_range_components::{UiMaximum, UiMinimum};
use super::authored_ui_node_projection_components::{UiNodeId, UiSliderValueChanged, UiValue};
use bevy::input_focus::InputFocus;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct UiSliderPolicy {
    pub increment: f32,
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

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiSliderValuePresentation {
    pub(super) label: AssetId,
    pub(super) minimum_width: u8,
    pub(super) decimal_places: u8,
    pub(super) percent_suffix: bool,
}

#[derive(Component, PartialEq, Eq)]
#[relationship(relationship_target = SliderValueLabels)]
pub(super) struct SliderValueLabelFor(Entity);

#[derive(Component)]
#[relationship_target(relationship = SliderValueLabelFor)]
pub(super) struct SliderValueLabels(Vec<Entity>);

#[allow(clippy::type_complexity)]
pub(super) fn bind_authored_slider_value_labels(
    mut commands: Commands,
    changed_labels: Query<
        Entity,
        (
            With<UiNodeId>,
            Or<(
                Added<Text>,
                Changed<UiNodeId>,
                Changed<ChildOf>,
                Added<crate::authored_ui_text_content_binding::UiTextBinding>,
            )>,
        ),
    >,
    changed_sliders: Query<Entity, Changed<UiSliderValuePresentation>>,
    mut unbound_sliders: RemovedComponents<SliderValueLabels>,
    mut unbound_text: RemovedComponents<crate::authored_ui_text_content_binding::UiTextBinding>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    labels: Query<
        &UiNodeId,
        (
            With<Text>,
            Without<crate::authored_ui_text_content_binding::UiTextBinding>,
        ),
    >,
    sliders: Query<(&UiSliderValuePresentation, Option<&SliderValueLabels>)>,
) {
    let mut dirty = bevy::ecs::entity::EntityHashSet::default();
    dirty.extend(changed_sliders.iter());
    dirty.extend(unbound_sliders.read());
    for label in changed_labels.iter().chain(unbound_text.read()) {
        dirty.extend(
            parents
                .iter_ancestors::<ChildOf>(label)
                .filter(|node| sliders.contains(*node)),
        );
    }
    for slider in dirty {
        let Ok((policy, current)) = sliders.get(slider) else {
            continue;
        };
        let desired = children
            .iter_descendants::<Children>(slider)
            .find(|node| labels.get(*node).is_ok_and(|id| id.id == policy.label));
        let old = current.and_then(|labels| labels.iter().next());
        if old == desired {
            continue;
        }
        if let Some(old) = old {
            commands.entity(old).try_remove::<SliderValueLabelFor>();
        }
        if let Some(label) = desired {
            commands.entity(label).insert(SliderValueLabelFor(slider));
        }
    }
}

pub(super) fn project_authored_slider_value_labels(
    sliders: Query<(
        Ref<UiSliderValuePresentation>,
        Ref<UiValue>,
        Ref<SliderValueLabels>,
    )>,
    mut texts: Query<&mut Text, Without<crate::authored_ui_text_content_binding::UiTextBinding>>,
) {
    for (policy, value, labels) in &sliders {
        if !(policy.is_changed() || value.is_changed() || labels.is_changed()) {
            continue;
        }
        let width = usize::from(policy.minimum_width);
        let mut formatted = if policy.decimal_places == 0 {
            format!("{:width$}", value.0)
        } else {
            let precision = usize::from(policy.decimal_places);
            format!("{:width$.precision$}", value.0 as f64)
        };
        if policy.percent_suffix {
            formatted.push('%');
        }
        for label in labels.iter() {
            if let Ok(text) = texts.get_mut(label) {
                text.map_unchanged(|text| &mut text.0)
                    .clone_from_if_neq(&formatted);
            }
        }
    }
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

pub(super) fn effective_axis(axis: UiSliderAxis, size: Vec2) -> UiSliderAxis {
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

/// The thumb's layout and inherited transform are still authored. Its slider
/// supplies only the translation along the authored centre interval.
#[derive(Component)]
#[relationship(relationship_target = AuthoredSliderThumbs)]
pub(super) struct AuthoredSliderThumb(Entity);

#[derive(Component)]
#[relationship_target(relationship = AuthoredSliderThumb)]
pub(super) struct AuthoredSliderThumbs(Vec<Entity>);

#[derive(Component, Default)]
pub(super) struct AuthoredSliderGrab(Option<Vec2>);

pub(super) fn link_authored_slider_thumbs(
    mut commands: Commands,
    sliders: Query<
        (
            Entity,
            &crate::authored_ui_node_projection_components::UiDocumentOwner,
            &UiSliderPresentation,
        ),
        Added<UiSliderPresentation>,
    >,
    targets: crate::authored_ui_target_lookup::AuthoredUiTargetLookup,
    parents: Query<&ChildOf>,
) {
    for (slider, owner, presentation) in &sliders {
        if presentation.thumb == AssetId::default() {
            continue;
        }
        if let Some(thumb) = targets
            .candidates(owner.0, presentation.thumb)
            .find(|entity| {
                parents
                    .iter_ancestors::<ChildOf>(*entity)
                    .any(|ancestor| ancestor == slider)
            })
        {
            commands.entity(thumb).insert(AuthoredSliderThumb(slider));
        }
    }
}

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

pub(super) fn project_authored_values_to_native_sliders(
    mut commands: Commands,
    sliders: Query<
        (
            Entity,
            &UiSliderPolicy,
            &UiMinimum,
            &UiMaximum,
            &UiValue,
            &SliderValue,
            &SliderRange,
            &SliderStep,
        ),
        (
            With<Slider>,
            Or<(
                Changed<UiValue>,
                Changed<UiMinimum>,
                Changed<UiMaximum>,
                Changed<UiSliderPolicy>,
            )>,
        ),
    >,
) {
    for (entity, policy, low, high, value, native, range, step) in &sliders {
        let (low, high) = ordered_bounds(low.0, high.0);
        let value = SliderValue(value.0 as f32);
        let limit = SliderRange::new(low as f32, high as f32);
        let increment = SliderStep(slider_step(policy) as f32);
        // Bevy's slider components are immutable and their insertion hooks
        // notify the thumb when the native configuration changes.
        let mut slider = commands.entity(entity);
        if range != &limit {
            slider.insert(limit);
        }
        if native != &value {
            slider.insert(value);
        }
        if step != &increment {
            slider.insert(increment);
        }
    }
}

pub(super) fn accept_slider_value(
    slider: Entity,
    value: &mut Mut<UiValue>,
    next: i64,
    edits: &mut MessageWriter<UiSliderValueChanged>,
) {
    if value.set_if_neq(UiValue(next)) {
        edits.write(UiSliderValueChanged {
            slider,
            value: next,
        });
    }
}

pub(super) fn adjust_focused_sliders(
    mut actions: MessageReader<ActionRequest>,
    focus: Res<InputFocus>,
    context: crate::active_authored_ui_context::ActiveAuthoredUiContext,
    scope: Res<crate::active_authored_ui_context::AuthoredFocusScope>,
    sliders: Query<(&UiSliderPolicy, Option<&ScrollsViewport>, Has<Slider>)>,
    mut slider_input: MessageWriter<UiSliderValueChanged>,
    mut values: Query<(&UiMinimum, &UiMaximum, &mut UiValue)>,
    mut scrolls: Query<(&ComputedNode, &mut ScrollPosition)>,
) {
    let root = scope.document;
    let modal = scope.modal;
    for request in actions.read() {
        let Some((focused, _)) = focus
            .get()
            .zip(root)
            .filter(|(focused, root)| context.eligible_in_scope(*focused, *root, modal))
        else {
            continue;
        };
        let Ok((policy, viewport, native)) = sliders.get(focused) else {
            continue;
        };
        if native
            && request.source
                == openzt2_interface_state::input::input_types::ActionSource::KeyboardMouse
        {
            continue;
        }
        let direction = match (policy.axis, request.action) {
            (UiSliderAxis::Horizontal | UiSliderAxis::Both, GameAction::NavigateLeft)
            | (UiSliderAxis::Vertical | UiSliderAxis::Both, GameAction::NavigateDown) => -1,
            (UiSliderAxis::Horizontal | UiSliderAxis::Both, GameAction::NavigateRight)
            | (UiSliderAxis::Vertical | UiSliderAxis::Both, GameAction::NavigateUp) => 1,
            _ => continue,
        };
        if let Some(viewport) = viewport {
            let Ok((bounds, mut offset)) = scrolls.get_mut(viewport.0) else {
                continue;
            };
            let direction = if matches!(
                request.action,
                GameAction::NavigateUp | GameAction::NavigateDown
            ) {
                -direction
            } else {
                direction
            };
            let limit = crate::authored_scrollbar::limit(bounds, policy.axis).round() as i64;
            let current = crate::authored_scrollbar::offset(&offset, policy.axis).round() as i64;
            let next = stepped_value(current, direction, policy, 0, limit);
            crate::authored_scrollbar::set_offset(
                &mut offset,
                policy.axis,
                next as f32,
                crate::authored_scrollbar::overflow(bounds),
            );
        } else if let Ok((minimum, maximum, mut value)) = values.get_mut(focused) {
            let next = stepped_value(value.0, direction, policy, minimum.0, maximum.0);
            accept_slider_value(focused, &mut value, next, &mut slider_input);
        }
    }
}

pub(super) fn apply_native_slider_value(
    event: On<ValueChange<f32>>,
    context: crate::active_authored_ui_context::ActiveAuthoredUiContext,
    modal: Option<
        Res<openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture>,
    >,
    mut slider_input: MessageWriter<UiSliderValueChanged>,
    mut values: Query<(&UiSliderPolicy, &UiMinimum, &UiMaximum, &mut UiValue), With<Slider>>,
) {
    let node = event.source;
    if !context.visible_and_enabled(node)
        || modal
            .and_then(|modal| modal.0)
            .is_some_and(|modal| !context.is_in_modal(node, modal))
    {
        return;
    }
    let Ok((policy, minimum, maximum, mut value)) = values.get_mut(node) else {
        return;
    };
    let next = clamp_and_step(f64::from(event.value), policy, minimum.0, maximum.0);
    accept_slider_value(node, &mut value, next, &mut slider_input);
}

pub(super) fn apply_authored_slider_pointer_events(
    mut presses: MessageReader<PointerPress>,
    mut drags: MessageReader<PointerDrag>,
    mut pointer_capture: ResMut<PointerCaptureMap>,
    context: crate::active_authored_ui_context::ActiveAuthoredUiContext,
    scope: Res<crate::active_authored_ui_context::AuthoredFocusScope>,
    parents: Query<&ChildOf>,
    windows: Query<&Window, With<PrimaryWindow>>,
    geometry: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut sliders: Query<
        (
            &UiSliderPresentation,
            &UiSliderPolicy,
            Option<&ScrollsViewport>,
            Option<&AuthoredSliderThumbs>,
            &mut AuthoredSliderGrab,
        ),
        Without<Slider>,
    >,
    mut slider_input: MessageWriter<UiSliderValueChanged>,
    mut values: Query<(&UiMinimum, &UiMaximum, &mut UiValue)>,
    mut scrolls: Query<(&ComputedNode, &mut ScrollPosition)>,
) {
    let modal = scope.modal;
    let factor = windows.single().map_or(1.0, Window::scale_factor);
    let presses = presses
        .read()
        .filter(|event| event.pointer.id.is_mouse() && event.button == PointerButton::Primary)
        .map(|event| (event.entity, event.pointer.position, Some(&event.hit)));
    let drags = drags
        .read()
        .filter(|event| event.pointer.id.is_mouse() && event.button == PointerButton::Primary)
        .map(|event| (event.entity, event.pointer.position, None));
    for (target, logical, press) in presses.chain(drags) {
        let Some(slider) = std::iter::once(target)
            .chain(parents.iter_ancestors::<ChildOf>(target))
            .find(|node| sliders.contains(*node))
        else {
            continue;
        };
        if !context.visible_and_enabled(slider)
            || modal.is_some_and(|modal| !context.is_in_modal(slider, modal))
        {
            continue;
        }
        let Ok((presentation, policy, viewport, thumbs, mut grab)) = sliders.get_mut(slider) else {
            continue;
        };
        let Ok((track, transform)) = geometry.get(presentation.track) else {
            continue;
        };
        let point = logical * factor;
        let thumb =
            thumbs.and_then(|thumbs| thumbs.iter().find_map(|thumb| geometry.get(thumb).ok()));
        if let Some(hit) = press {
            grab.0 = if let Some((_, transform)) =
                thumb.filter(|(node, transform)| node.contains_point(**transform, point))
            {
                Some(point - transform.affine().translation)
            } else if track.contains_point(*transform, point) {
                Some(Vec2::ZERO)
            } else {
                None
            };
            if grab.0.is_some() {
                pointer_capture.capture(PointerId::Mouse, target, hit.clone());
            }
        }
        let Some(offset) = grab.0 else { continue };
        if let Some(viewport) = viewport {
            let Ok((scroll, mut position)) = scrolls.get_mut(viewport.0) else {
                continue;
            };
            let axis = effective_axis(policy.axis, track.size());
            let thumb = thumb.map_or(Vec2::ZERO, |(node, _)| node.size());
            let thumb_length = if axis == UiSliderAxis::Horizontal {
                thumb.x
            } else {
                thumb.y
            };
            let limit = crate::authored_scrollbar::limit(scroll, axis).round() as i64;
            if let Some(next) = value_from_point(
                point - offset,
                track,
                *transform,
                policy,
                Some(thumb_length),
                0,
                limit,
            ) {
                crate::authored_scrollbar::set_offset(
                    &mut position,
                    axis,
                    next as f32,
                    crate::authored_scrollbar::overflow(scroll),
                );
            }
        } else if let Ok((minimum, maximum, mut value)) = values.get_mut(slider) {
            if let Some(next) = value_from_point(
                point - offset,
                track,
                *transform,
                policy,
                None,
                minimum.0,
                maximum.0,
            ) {
                accept_slider_value(slider, &mut value, next, &mut slider_input);
            }
        }
    }
}

pub(super) fn project_slider_thumb_layout(
    sliders: Query<(
        &UiSliderPresentation,
        Ref<UiSliderPolicy>,
        Option<&ScrollsViewport>,
    )>,
    values: Query<(Ref<UiMinimum>, Ref<UiMaximum>, Ref<UiValue>)>,
    scrolls: Query<(Ref<ComputedNode>, Ref<ScrollPosition>)>,
    geometry: Query<(Ref<ComputedNode>, Ref<UiGlobalTransform>)>,
    mut thumbs: Query<(
        Entity,
        Ref<AuthoredSliderThumb>,
        &ChildOf,
        &mut UiTransform,
        &mut Node,
    )>,
) {
    for (entity, thumb, parent, mut input, mut layout) in &mut thumbs {
        let Ok((presentation, policy, viewport)) = sliders.get(thumb.0) else {
            continue;
        };
        let (
            Ok((track, track_transform)),
            Ok((thumb_size, thumb_transform)),
            Ok((_, parent_transform)),
        ) = (
            geometry.get(presentation.track),
            geometry.get(entity),
            geometry.get(parent.parent()),
        )
        else {
            continue;
        };
        let axis = effective_axis(policy.axis, track.size());
        let horizontal = axis == UiSliderAxis::Horizontal;
        let track_length = if horizontal {
            track.size().x
        } else {
            track.size().y
        };
        let geometry_changed = thumb.is_changed()
            || policy.is_changed()
            || track.is_changed()
            || track_transform.is_changed()
            || thumb_size.is_changed()
            || thumb_transform.is_changed();
        let (ratio, travel) = if let Some(viewport) = viewport {
            let Ok((bounds, position)) = scrolls.get(viewport.0) else {
                continue;
            };
            if !geometry_changed && !bounds.is_changed() && !position.is_changed() {
                continue;
            }
            let extent = if horizontal {
                bounds.size().x
            } else {
                bounds.size().y
            };
            let contents = if horizontal {
                bounds.content_size().x
            } else {
                bounds.content_size().y
            };
            let proportion = if contents > extent {
                extent / contents
            } else {
                1.0
            };
            let minimum = (policy.minimum_thumb_size.max(0.0) / track.inverse_scale_factor())
                .min(track_length);
            let length = (track_length * proportion).clamp(minimum, track_length);
            let edge = if horizontal {
                &mut layout.width
            } else {
                &mut layout.height
            };
            let size = px(length * track.inverse_scale_factor());
            if *edge != size {
                *edge = size;
            }
            let limit = crate::authored_scrollbar::limit(&bounds, axis);
            let ratio = if limit > 0.0 {
                (crate::authored_scrollbar::offset(&position, axis) / limit).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (ratio, (track_length - length).max(0.0))
        } else {
            let Ok((low, high, value)) = values.get(thumb.0) else {
                continue;
            };
            if !geometry_changed && !low.is_changed() && !high.is_changed() && !value.is_changed() {
                continue;
            }
            let ratio = value_ratio(value.0, low.0, high.0);
            (if horizontal { ratio } else { 1.0 - ratio }, track_length)
        };
        let (Some(inverse_track), Some(inverse_parent)) = (
            track_transform.try_inverse(),
            parent_transform.try_inverse(),
        ) else {
            continue;
        };
        let current = thumb_transform.affine().translation;
        let mut centre = inverse_track.transform_point2(current);
        if horizontal {
            centre.x = (ratio - 0.5) * travel;
        } else {
            centre.y = (ratio - 0.5) * travel;
        }
        let delta = inverse_parent
            .transform_vector2(track_transform.affine().transform_point2(centre) - current)
            * thumb_size.inverse_scale_factor();
        if delta.abs().max_element() <= 0.001 {
            continue;
        }
        let pixels = |value| {
            if let Val::Px(value) = value {
                value
            } else {
                0.0
            }
        };
        input.translation = Val2::px(
            pixels(input.translation.x) + delta.x,
            pixels(input.translation.y) + delta.y,
        );
    }
}
