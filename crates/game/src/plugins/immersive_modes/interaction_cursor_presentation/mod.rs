use bevy::{picking::Pickable, prelude::*};

use super::immersive_mode_policy_types::{InteractionCursor, InteractionCursorVisual};

pub(super) fn move_immersive_mode_cursor_visual_to_current_pointer_position(
    pointer_input: Res<crate::plugins::input::input_types::PrimaryPointerInputState>,
    mut cursor_visual_nodes: Query<&mut Node, With<InteractionCursorVisual>>,
) {
    if !pointer_input.is_changed() {
        return;
    }
    for mut cursor_visual_node in &mut cursor_visual_nodes {
        cursor_visual_node.left = Val::Px(pointer_input.screen.x);
        cursor_visual_node.top = Val::Px(pointer_input.screen.y);
    }
}

/// Displays the cursor image selected by the active immersive mode.
pub(super) fn create_cursor_visual_for_changed_immersive_mode_cursor_policy(
    changed_cursor_policies: Query<(Entity, &InteractionCursor), Changed<InteractionCursor>>,
    existing_cursor_visuals: Query<(Entity, &InteractionCursorVisual)>,
    mut commands: Commands,
) {
    for (controller_entity, interaction_cursor) in &changed_cursor_policies {
        for (cursor_visual_entity, cursor_visual) in &existing_cursor_visuals {
            if cursor_visual.owner == controller_entity {
                commands.entity(cursor_visual_entity).despawn();
            }
        }
        commands.spawn((
            InteractionCursorVisual {
                owner: controller_entity,
            },
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            ImageNode::new(interaction_cursor.image.clone()),
            GlobalZIndex(i32::MAX),
            Pickable::IGNORE,
        ));
    }
}

pub(super) fn retire_cursor_visuals_without_immersive_mode_cursor_owner(
    cursor_policy_owners: Query<(), With<InteractionCursor>>,
    cursor_visuals: Query<(Entity, &InteractionCursorVisual)>,
    mut commands: Commands,
) {
    for (cursor_visual_entity, cursor_visual) in &cursor_visuals {
        if cursor_policy_owners.get(cursor_visual.owner).is_err() {
            commands.entity(cursor_visual_entity).despawn();
        }
    }
}

pub(super) fn retire_all_immersive_mode_cursor_visuals_when_leaving_gameplay(
    cursor_visuals: Query<Entity, With<InteractionCursorVisual>>,
    mut commands: Commands,
) {
    for cursor_visual_entity in &cursor_visuals {
        commands.entity(cursor_visual_entity).despawn();
    }
}
