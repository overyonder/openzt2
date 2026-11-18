use bevy::{picking::Pickable, prelude::*};

use super::immersive_mode_policy_types::{
    InteractionCursor, InteractionCursorVisual, InteractionCursorVisuals,
};

pub(super) fn move_immersive_mode_cursor_visual_to_current_pointer_position(
    pointer_input: Res<openzt2_interface_state::input::input_types::PrimaryPointerInputState>,
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

pub(super) fn project_changed_immersive_mode_cursor_images(
    mut commands: Commands,
    pointer: Res<openzt2_interface_state::input::input_types::PrimaryPointerInputState>,
    policies: Query<
        (
            Entity,
            &InteractionCursor,
            Option<&InteractionCursorVisuals>,
        ),
        Changed<InteractionCursor>,
    >,
    mut visuals: Query<&mut ImageNode, With<InteractionCursorVisual>>,
) {
    for (controller, cursor, current) in &policies {
        if let Some(mut image) = current
            .and_then(|current| current.iter().next())
            .and_then(|entity| visuals.get_mut(entity).ok())
        {
            image
                .reborrow()
                .map_unchanged(|image| &mut image.image)
                .set_if_neq(cursor.image.clone());
            continue;
        }
        commands.spawn((
            InteractionCursorVisual(controller),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(pointer.screen.x),
                top: Val::Px(pointer.screen.y),
                ..default()
            },
            ImageNode::new(cursor.image.clone()),
            GlobalZIndex(i32::MAX),
            Pickable::IGNORE,
        ));
    }
}

pub(super) fn remove_immersive_cursor_on_policy_removal(
    removed: On<Remove<InteractionCursor>>,
    children: Query<&InteractionCursorVisuals>,
    mut commands: Commands,
) {
    if let Ok(children) = children.get(removed.entity) {
        for entity in children.iter() {
            commands.entity(entity).try_despawn();
        }
    }
}
