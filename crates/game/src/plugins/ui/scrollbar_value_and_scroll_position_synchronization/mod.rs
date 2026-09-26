use bevy::prelude::*;

use super::{
    authored_ui_integer_range_components::{UiMaximum, UiMinimum},
    authored_ui_node_projection_components::UiValue,
    slider::UiSliderAxis,
};

/// Connects a scrollbar to the list or window it scrolls.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiScrollDecoration {
    pub(crate) owner: Entity,
    pub(crate) axis: UiSliderAxis,
}

/// Applies a changed scrollbar value to its target scroll offset.
pub(super) fn apply_changed_authored_scrollbar_values_to_owned_bevy_scroll_positions(
    decorations: Query<(&UiScrollDecoration, Ref<UiValue>), Changed<UiValue>>,
    bounds: Query<&ComputedNode>,
    mut owners: Query<&mut ScrollPosition>,
) {
    for (decoration, value) in &decorations {
        let (Ok(computed), Ok(mut position)) = (
            bounds.get(decoration.owner),
            owners.get_mut(decoration.owner),
        ) else {
            continue;
        };
        let maximum = (computed.content_size() - computed.size()).max(Vec2::ZERO)
            * computed.inverse_scale_factor();
        let value = value.0 as f32;
        match decoration.axis {
            UiSliderAxis::Horizontal => position.x = value.clamp(0.0, maximum.x),
            UiSliderAxis::Vertical => position.y = value.clamp(0.0, maximum.y),
            UiSliderAxis::Both => {
                position.x = value.clamp(0.0, maximum.x);
                position.y = value.clamp(0.0, maximum.y);
            }
        }
    }
}

/// Updates scrollbar bounds and value from the current overflow and scroll offset.
pub(super) fn project_owned_bevy_scroll_positions_into_authored_scrollbar_values(
    owners: Query<(&ComputedNode, &ScrollPosition)>,
    mut decorations: Query<(
        &UiScrollDecoration,
        &mut UiMinimum,
        &mut UiMaximum,
        &mut UiValue,
    )>,
) {
    for (decoration, mut minimum, mut maximum, mut value) in &mut decorations {
        let Ok((computed, position)) = owners.get(decoration.owner) else {
            continue;
        };
        let overflow = (computed.content_size() - computed.size()).max(Vec2::ZERO)
            * computed.inverse_scale_factor();
        let (limit, offset) = match decoration.axis {
            UiSliderAxis::Horizontal => (overflow.x, position.x),
            UiSliderAxis::Vertical => (overflow.y, position.y),
            UiSliderAxis::Both => (overflow.max_element(), position.x.max(position.y)),
        };
        let limit = limit.round() as i64;
        let offset = offset.clamp(0.0, limit as f32).round() as i64;
        if minimum.0 != 0 {
            minimum.0 = 0;
        }
        if maximum.0 != limit {
            maximum.0 = limit;
        }
        if value.0 != offset {
            value.0 = offset;
        }
    }
}

pub(super) fn present_authored_scrollbar_decorations_from_owned_bevy_overflow(
    owners: Query<(&ComputedNode, &Node)>,
    mut decorations: Query<(&UiScrollDecoration, &mut Visibility)>,
) {
    for (decoration, mut visibility) in &mut decorations {
        let Ok((computed, node)) = owners.get(decoration.owner) else {
            continue;
        };
        // Bevy's layout pass owns and mutates `ComputedNode`, but those
        // internal layout writes do not promise an ECS `Changed` tick. Derive
        // the small authored scrollbar set from the current native bounds
        // every frame and only write visibility when its value differs.
        let overflow = computed.content_size() - computed.size();
        let visible = match decoration.axis {
            UiSliderAxis::Horizontal => node.overflow.x == OverflowAxis::Scroll && overflow.x > 0.5,
            UiSliderAxis::Vertical => node.overflow.y == OverflowAxis::Scroll && overflow.y > 0.5,
            UiSliderAxis::Both => {
                (node.overflow.x == OverflowAxis::Scroll && overflow.x > 0.5)
                    || (node.overflow.y == OverflowAxis::Scroll && overflow.y > 0.5)
            }
        };
        let next = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != next {
            *visibility = next;
        }
    }
}
