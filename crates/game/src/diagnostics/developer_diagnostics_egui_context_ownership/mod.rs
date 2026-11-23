use bevy::prelude::*;
use bevy_inspector_egui::bevy_egui::{EguiContext, EguiSchedule, PrimaryEguiContext};
use openzt2_interface_state::shell::shell_screen_presentation_types::ShellUiCamera;

#[derive(Message)]
pub(super) struct ShellCameraMembershipChanged;

pub(super) fn shell_camera_added(
    _: On<Add<ShellUiCamera>>,
    mut changes: MessageWriter<ShellCameraMembershipChanged>,
) {
    changes.write(ShellCameraMembershipChanged);
}

pub(super) fn shell_camera_removed(
    _: On<Remove<ShellUiCamera>>,
    mut changes: MessageWriter<ShellCameraMembershipChanged>,
) {
    changes.write(ShellCameraMembershipChanged);
}

pub(super) fn reconcile_primary_egui_context(
    mut commands: Commands,
    shell_cameras: Query<(Entity, Has<PrimaryEguiContext>), With<ShellUiCamera>>,
    primary_contexts: Query<Entity, With<PrimaryEguiContext>>,
) {
    let target = shell_cameras
        .iter()
        .min_by_key(|(entity, _)| entity.index());
    for context in &primary_contexts {
        if target.is_none_or(|(target, _)| target != context) {
            commands
                .entity(context)
                .remove::<(PrimaryEguiContext, EguiSchedule, EguiContext)>();
        }
    }
    if let Some((target, false)) = target {
        commands.entity(target).insert(PrimaryEguiContext);
    }
}
