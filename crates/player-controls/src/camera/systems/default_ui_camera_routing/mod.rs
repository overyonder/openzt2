use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_interface_state::shell::shell_screen_presentation_types::ShellClearCamera;
use openzt2_interface_state::shell::shell_screen_presentation_types::ShellUiCamera;

use super::super::camera_runtime_state_types::ZooCamera;

/// Use the zoo camera as Bevy's single world-and-UI camera during gameplay.
/// Shell screens retain their dedicated 2D camera before a world exists, so
/// two cameras never compete to composite the live game target.
pub(crate) fn route_default_ui_camera(
    phase: Res<State<GamePhase>>,
    added: Query<
        (),
        (
            Or<(With<ZooCamera>, With<ShellUiCamera>, With<ShellClearCamera>)>,
            Or<(
                Added<Camera>,
                Added<ZooCamera>,
                Added<ShellUiCamera>,
                Added<ShellClearCamera>,
            )>,
        ),
    >,
    mut removed_zoo_cameras: RemovedComponents<ZooCamera>,
    mut removed_cameras: RemovedComponents<Camera>,
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
    let removed = !removed_zoo_cameras.is_empty() || !removed_cameras.is_empty();
    removed_zoo_cameras.clear();
    removed_cameras.clear();
    if !phase.is_changed() && !removed && added.is_empty() {
        return;
    }
    let use_zoo = *phase.get() == GamePhase::InGame && !zoo_cameras.is_empty();
    for (entity, camera, is_default) in &mut shell_cameras {
        camera
            .map_unchanged(|camera| &mut camera.is_active)
            .set_if_neq(!use_zoo);
        if use_zoo && is_default {
            commands.entity(entity).remove::<IsDefaultUiCamera>();
        } else if !use_zoo && !is_default {
            commands.entity(entity).insert(IsDefaultUiCamera);
        }
    }
    for (entity, camera, is_default) in &mut zoo_cameras {
        camera
            .map_unchanged(|camera| &mut camera.is_active)
            .set_if_neq(use_zoo);
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
