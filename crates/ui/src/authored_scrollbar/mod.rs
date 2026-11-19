use bevy::prelude::*;

use crate::{
    authored_ui_change_activation_dispatch::apply_visibility_transition,
    slider::{effective_axis, UiSliderAxis, UiSliderPolicy},
};
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

#[derive(Component)]
#[relationship(relationship_target = ViewportScrollbars)]
pub(crate) struct ScrollsViewport(pub(crate) Entity);

#[derive(Component)]
#[relationship_target(relationship = ScrollsViewport)]
pub(crate) struct ViewportScrollbars(Vec<Entity>);

pub(super) fn overflow(node: &ComputedNode) -> Vec2 {
    (node.content_size() - node.size()).max(Vec2::ZERO) * node.inverse_scale_factor()
}

pub(super) fn set_offset(
    position: &mut ScrollPosition,
    axis: UiSliderAxis,
    value: f32,
    bounds: Vec2,
) {
    match axis {
        UiSliderAxis::Horizontal => position.x = value.clamp(0.0, bounds.x),
        UiSliderAxis::Vertical => position.y = value.clamp(0.0, bounds.y),
        UiSliderAxis::Both => position.0 = Vec2::splat(value).clamp(Vec2::ZERO, bounds),
    }
}

pub(super) fn offset(position: &ScrollPosition, axis: UiSliderAxis) -> f32 {
    match axis {
        UiSliderAxis::Horizontal => position.x,
        UiSliderAxis::Vertical => position.y,
        UiSliderAxis::Both => position.x.max(position.y),
    }
}

pub(super) fn limit(node: &ComputedNode, axis: UiSliderAxis) -> f32 {
    let bounds = overflow(node);
    match axis {
        UiSliderAxis::Horizontal => bounds.x,
        UiSliderAxis::Vertical => bounds.y,
        UiSliderAxis::Both => bounds.max_element(),
    }
}

pub(super) fn project_scrollbar_availability(
    owners: Query<(Ref<Node>, Ref<ComputedNode>)>,
    mut scrollbars: Query<(
        Entity,
        Ref<ScrollsViewport>,
        Ref<UiSliderPolicy>,
        Ref<ComputedNode>,
        &mut Visibility,
    )>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    for (entity, viewport, slider, track, mut visibility) in &mut scrollbars {
        let Ok((node, computed)) = owners.get(viewport.0) else {
            continue;
        };
        if !viewport.is_changed()
            && !slider.is_changed()
            && !track.is_changed()
            && !node.is_changed()
            && !computed.is_changed()
        {
            continue;
        }
        let axis = effective_axis(slider.axis, track.size());
        let visible_horizontal = node.overflow.x == OverflowAxis::Scroll
            && computed.content_size().x - computed.size().x > 0.5;
        let visible_vertical = node.overflow.y == OverflowAxis::Scroll
            && computed.content_size().y - computed.size().y > 0.5;
        let visible = match slider.axis {
            UiSliderAxis::Both => visible_horizontal || visible_vertical,
            _ if axis == UiSliderAxis::Horizontal => visible_horizontal,
            _ => visible_vertical,
        };
        apply_visibility_transition(
            entity,
            &mut visibility,
            if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
            true,
            &mut activated,
        );
    }
}
