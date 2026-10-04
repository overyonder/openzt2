use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    node::{UiNodeDefinition, UiNodeFlags, UiNodeKind},
    node_presentation::{UiNodePointerHitPolicy, UiNodeVisualState},
};

pub(super) fn initial_bevy_pickable_for_authored_node(node: &UiNodeDefinition) -> Pickable {
    // Decorative layers must never occlude an actionable sibling. Bevy's UI
    // picker stops at the uppermost hit even when that entity has no
    // `Interaction`; the original UI treated such layers as presentation only.
    if node.flags.0 & UiNodeFlags::POINTER_BLOCKING.0 == 0
        || (!authored_node_has_actions(node)
            && !authored_node_kind_owns_pointer_interaction(&node.kind))
    {
        return Pickable::IGNORE;
    }
    let policy = node
        .visuals
        .iter()
        .find(|visual| matches!(&visual.visual_state, UiNodeVisualState::Normal))
        .or_else(|| {
            node.visuals
                .iter()
                .find(|visual| matches!(&visual.visual_state, UiNodeVisualState::Default))
        })
        .map(|visual| &visual.hit_policy);
    // A list without a drawn surface has no pixels for the normal hit policy.
    // Its rows remain targets and its wheel viewport stays rectangular, but its
    // empty area must not cover controls beneath it, such as the main menu's
    // Exit button under the empty expansion icon strip.
    let undrawn_list = matches!(node.kind, UiNodeKind::List)
        && !matches!(policy, Some(UiNodePointerHitPolicy::Always))
        && node.visuals.iter().all(|visual| {
            visual.image.0 == [0; 16] && (!visual.affects_color || visual.color[3] <= 0.0)
        });
    if undrawn_list || matches!(policy, Some(UiNodePointerHitPolicy::Never)) {
        Pickable::IGNORE
    } else {
        Pickable::default()
    }
}

pub(super) fn authored_node_kind_owns_pointer_interaction(kind: &UiNodeKind) -> bool {
    matches!(
        kind,
        UiNodeKind::Button
            | UiNodeKind::Toggle
            | UiNodeKind::Slider
            | UiNodeKind::Scroll
            | UiNodeKind::List
            | UiNodeKind::Edit
            | UiNodeKind::CompositeButton
            | UiNodeKind::Drag
            | UiNodeKind::DragCommand
            | UiNodeKind::Globe
            | UiNodeKind::Tool
            | UiNodeKind::TreeElement
            | UiNodeKind::ContextList
            | UiNodeKind::FullscreenButton
            | UiNodeKind::TypeList
            | UiNodeKind::Window
            | UiNodeKind::XmlEdit
    )
}

pub(super) fn authored_node_has_actions(node: &UiNodeDefinition) -> bool {
    !node.actions.is_empty()
}
