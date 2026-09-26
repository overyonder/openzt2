use bevy::prelude::*;
use bevy_inspector_egui::bevy_egui::{EguiContext, EguiMultipassSchedule, PrimaryEguiContext};

use crate::plugins::shell::shell_screen_presentation_types::ShellUiCamera;

/// Keeps the diagnostics UI on the game's single UI camera.
///
/// A `PrimaryEguiContext` uses Bevy Egui's primary multipass schedule, so two
/// marked cameras are invalid. Shell transitions may replace their camera
/// across deferred command boundaries; repair ownership before Egui begins its
/// pass instead of allowing two contexts to share that schedule.
pub(super) fn own_primary_egui_context_on_shell_camera(
    mut commands: Commands,
    shell_cameras: Query<(Entity, Has<PrimaryEguiContext>), With<ShellUiCamera>>,
    primary_contexts: Query<Entity, With<PrimaryEguiContext>>,
) {
    let target = shell_cameras
        .iter()
        .min_by_key(|(entity, _)| entity.index());
    let Some((target, target_is_primary)) = target else {
        return;
    };

    for context in &primary_contexts {
        if context != target {
            commands
                .entity(context)
                .remove::<(PrimaryEguiContext, EguiMultipassSchedule, EguiContext)>();
        }
    }

    if !target_is_primary {
        commands.entity(target).insert(PrimaryEguiContext);
    }
}
