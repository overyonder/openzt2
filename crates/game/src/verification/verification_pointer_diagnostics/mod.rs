use bevy::prelude::*;

use crate::plugins::{
    camera::world_pointer_ray_types::WorldPointerRay,
    construction::construction_interaction_types::{ConstructionCursor, ConstructionPreview},
    ui::picking::UiPointerCapture,
};

/// Names what each scripted click landed on, so a missed click is obvious in the log.
pub(crate) fn log_verification_pointer_click_target(
    event: On<Pointer<Click>>,
    names: Query<&Name>,
) {
    if event.original_event_target() == event.event_target() {
        info!(target: "openzt2_verification",
            name = names.get(event.event_target()).map_or("<unnamed>", Name::as_str),
            "click landed");
    }
}

/// Logs changes to the state that decides whether world placement can see the pointer.
pub(crate) fn log_world_pointer_and_placement_cursor_changes(
    ui_pointer_capture: Res<UiPointerCapture>,
    world_pointer_ray: Res<WorldPointerRay>,
    cursors: Query<&ConstructionCursor>,
    previews: Query<&Visibility, With<ConstructionPreview>>,
    names: Query<&Name>,
    mut previous: Local<Option<(bool, Option<Entity>, bool, bool, usize, usize)>>,
) {
    let current = (
        ui_pointer_capture.over_ui,
        ui_pointer_capture.target,
        world_pointer_ray.0.is_some(),
        cursors.iter().any(|cursor| cursor.over_terrain),
        previews.iter().count(),
        previews
            .iter()
            .filter(|visibility| **visibility != Visibility::Hidden)
            .count(),
    );
    if previous.replace(current) == Some(current) {
        return;
    }
    info!(target: "openzt2_verification",
        over_ui = current.0,
        capture_target = current.1.and_then(|entity| names.get(entity).ok()).map_or("<none>", Name::as_str),
        ray = current.2,
        over_terrain = current.3,
        previews = current.4,
        visible_previews = current.5,
        "world pointer changed");
}
