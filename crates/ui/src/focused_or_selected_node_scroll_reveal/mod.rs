use bevy::prelude::*;

use super::authored_ui_focus_state::UiFocusScope;
use super::authored_ui_node_projection_components::UiDocumentOwner;
use super::authored_ui_selection_state::UiSelected;

/// Scrolls the nearest ancestor to reveal the focused or selected node after layout.
pub(super) fn scroll_nearest_ancestor_to_reveal_focused_or_selected_node(
    scopes: Query<&UiFocusScope, Changed<UiFocusScope>>,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    selected: Query<(Entity, &UiSelected), Changed<UiSelected>>,
    parents: Query<&ChildOf>,
    nodes: Query<(
        &ComputedNode,
        &UiGlobalTransform,
        &UiDocumentOwner,
        &InheritedVisibility,
    )>,
    mut scrolls: Query<(
        &Node,
        &ComputedNode,
        &UiGlobalTransform,
        &UiDocumentOwner,
        &mut ScrollPosition,
    )>,
) {
    for (entity, selected) in &selected {
        if selected.0 {
            scroll_nearest_ancestor_to_reveal_node(
                entity,
                &parents,
                &nodes,
                &mut scrolls,
                &context,
            );
        }
    }
    // Focus is the narrower user intent when a domain presenter changes
    // selection and focus in the same frame, so apply it last.
    for scope in &scopes {
        if let Some(focused) = scope.focused {
            scroll_nearest_ancestor_to_reveal_node(
                focused,
                &parents,
                &nodes,
                &mut scrolls,
                &context,
            );
        }
    }
}

fn scroll_nearest_ancestor_to_reveal_node(
    target: Entity,
    parents: &Query<&ChildOf>,
    nodes: &Query<(
        &ComputedNode,
        &UiGlobalTransform,
        &UiDocumentOwner,
        &InheritedVisibility,
    )>,
    scrolls: &mut Query<(
        &Node,
        &ComputedNode,
        &UiGlobalTransform,
        &UiDocumentOwner,
        &mut ScrollPosition,
    )>,
    context: &super::active_authored_ui_context::ActiveAuthoredUiContext,
) {
    let Ok((target_node, target_transform, target_owner, visibility)) = nodes.get(target) else {
        return;
    };
    if !visibility.get() {
        return;
    }

    let target_half_size = target_node.size() * 0.5;
    let target_corners = [
        -target_half_size,
        Vec2::new(target_half_size.x, -target_half_size.y),
        target_half_size,
        Vec2::new(-target_half_size.x, target_half_size.y),
    ]
    .map(|corner| target_transform.transform_point2(corner));
    let mut ancestor = target;

    while let Ok(parent) = parents.get(ancestor) {
        ancestor = parent.parent();
        if let Ok((
            viewport_style,
            viewport_node,
            viewport_transform,
            viewport_owner,
            mut position,
        )) = scrolls.get_mut(ancestor)
        {
            if context.document_scope(viewport_owner.0) != context.document_scope(target_owner.0) {
                return;
            }
            if viewport_style.overflow.x != OverflowAxis::Scroll
                && viewport_style.overflow.y != OverflowAxis::Scroll
            {
                continue;
            }
            // Compare both extents in the viewport's coordinates. Canvas scaling
            // and ancestor transforms have already been applied to world bounds.
            let viewport_inverse = viewport_transform.affine().inverse();
            let local_corners =
                target_corners.map(|corner| viewport_inverse.transform_point2(corner));
            if local_corners.iter().any(|corner| !corner.is_finite()) {
                return;
            }
            let target_min = local_corners
                .into_iter()
                .fold(Vec2::splat(f32::INFINITY), Vec2::min);
            let target_max = local_corners
                .into_iter()
                .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
            let viewport_half_size = viewport_node.size() * 0.5;
            let viewport_min = -viewport_half_size;
            let viewport_max = viewport_half_size;
            let correction = Vec2::new(
                if viewport_style.overflow.x == OverflowAxis::Scroll {
                    calculate_axis_scroll_correction_to_reveal_extent(
                        target_min.x,
                        target_max.x,
                        viewport_min.x,
                        viewport_max.x,
                    )
                } else {
                    0.0
                },
                if viewport_style.overflow.y == OverflowAxis::Scroll {
                    calculate_axis_scroll_correction_to_reveal_extent(
                        target_min.y,
                        target_max.y,
                        viewport_min.y,
                        viewport_max.y,
                    )
                } else {
                    0.0
                },
            ) * viewport_node.inverse_scale_factor();
            let maximum = (viewport_node.content_size() - viewport_node.size()).max(Vec2::ZERO)
                * viewport_node.inverse_scale_factor();
            position.0 = (position.0 + correction).clamp(Vec2::ZERO, maximum);
            return;
        }
        if ancestor == context.document_scope(target_owner.0) {
            return;
        }
    }
}

fn calculate_axis_scroll_correction_to_reveal_extent(
    target_minimum: f32,
    target_maximum: f32,
    viewport_minimum: f32,
    viewport_maximum: f32,
) -> f32 {
    if target_minimum < viewport_minimum {
        target_minimum - viewport_minimum
    } else if target_maximum > viewport_maximum {
        target_maximum - viewport_maximum
    } else {
        0.0
    }
}
