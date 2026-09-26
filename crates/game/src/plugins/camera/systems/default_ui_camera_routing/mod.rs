use bevy::prelude::*;

use crate::{
    application_lifecycle::GamePhase,
    plugins::shell::shell_screen_presentation_types::{ShellClearCamera, ShellUiCamera},
};

use super::super::camera_runtime_state_types::ZooCamera;

/// Use the zoo camera as Bevy's single world-and-UI camera during gameplay.
/// Shell screens retain their dedicated 2D camera before a world exists, so
/// two cameras never compete to composite the live game target.
pub(in crate::plugins::camera) fn route_default_ui_camera(
    phase: Res<State<GamePhase>>,
    mut commands: Commands,
    mut shell_cameras: Query<
        (Entity, &mut Camera, Has<IsDefaultUiCamera>),
        (With<ShellUiCamera>, Without<ZooCamera>),
    >,
    mut zoo_cameras: Query<
        (Entity, &mut Camera, Has<IsDefaultUiCamera>),
        (With<ZooCamera>, Without<ShellUiCamera>),
    >,
    mut shell_clear_cameras: Query<
        &mut Camera,
        (
            With<ShellClearCamera>,
            Without<ShellUiCamera>,
            Without<ZooCamera>,
        ),
    >,
) {
    let use_zoo = *phase.get() == GamePhase::InGame && !zoo_cameras.is_empty();
    for (entity, mut camera, is_default) in &mut shell_cameras {
        camera.is_active = !use_zoo;
        if use_zoo && is_default {
            commands.entity(entity).remove::<IsDefaultUiCamera>();
        } else if !use_zoo && !is_default {
            commands.entity(entity).insert(IsDefaultUiCamera);
        }
    }
    for (entity, mut camera, is_default) in &mut zoo_cameras {
        camera.is_active = use_zoo;
        if use_zoo && !is_default {
            commands.entity(entity).insert(IsDefaultUiCamera);
        } else if !use_zoo && is_default {
            commands.entity(entity).remove::<IsDefaultUiCamera>();
        }
    }
    for mut camera in &mut shell_clear_cameras {
        if camera.is_active == use_zoo {
            camera.is_active = !use_zoo;
        }
    }
}
