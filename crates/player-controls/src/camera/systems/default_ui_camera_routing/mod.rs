use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_interface_state::shell::shell_screen_presentation_types::{
    ShellClearCamera, ShellUiCamera,
};
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

use super::super::camera_runtime_state_types::ZooCamera;

type CameraRole = Or<(With<ZooCamera>, With<ShellUiCamera>, With<ShellClearCamera>)>;

/// Route only one camera to the default UI surface, with the shell taking
/// over during loading or whenever there is no usable zoo camera.
pub(crate) fn route_default_ui_camera(
    phase: Res<State<GamePhase>>,
    roots: Query<(), With<WorldRoot>>,
    changed_roles: Query<
        (),
        (
            With<Camera>,
            CameraRole,
            Or<(
                Added<Camera>,
                Added<ZooCamera>,
                Added<ShellUiCamera>,
                Added<ShellClearCamera>,
                Changed<WorldMember>,
            )>,
        ),
    >,
    role_entities: Query<(), CameraRole>,
    mut removed_zoo: RemovedComponents<ZooCamera>,
    mut removed_ui: RemovedComponents<ShellUiCamera>,
    mut removed_clear: RemovedComponents<ShellClearCamera>,
    mut removed_cameras: RemovedComponents<Camera>,
    mut removed_worlds: RemovedComponents<WorldMember>,
    mut cameras: ParamSet<(
        Query<
            (
                Entity,
                Has<ZooCamera>,
                Has<ShellUiCamera>,
                Option<&WorldMember>,
            ),
            (With<Camera>, CameraRole),
        >,
        Query<(
            Entity,
            &mut Camera,
            Has<ZooCamera>,
            Has<ShellUiCamera>,
            Has<ShellClearCamera>,
            Has<IsDefaultUiCamera>,
        )>,
    )>,
    mut commands: Commands,
) {
    let removed_roles = removed_zoo
        .read()
        .chain(removed_ui.read())
        .chain(removed_clear.read())
        .collect::<bevy::ecs::entity::EntityHashSet>();
    let camera_removed = removed_cameras
        .read()
        .chain(removed_worlds.read())
        .fold(false, |removed, entity| {
            removed || role_entities.contains(entity)
        });
    if !phase.is_changed()
        && removed_roles.is_empty()
        && !camera_removed
        && changed_roles.is_empty()
    {
        return;
    }
    let candidates = cameras.p0();
    let zoo = (*phase.get() == GamePhase::InGame)
        .then(|| {
            candidates
                .iter()
                .filter(|(_, zoo, _, world)| {
                    *zoo && world.is_some_and(|world| roots.contains(world.root))
                })
                .map(|(entity, ..)| entity)
                .min_by_key(|entity| entity.to_bits())
        })
        .flatten();
    let shell = candidates
        .iter()
        .filter(|(_, _, ui, _)| *ui)
        .map(|(entity, ..)| entity)
        .min_by_key(|entity| entity.to_bits());
    let target = zoo.or(shell);

    for (entity, camera, is_zoo, is_shell, is_clear, is_default) in &mut cameras.p1() {
        if !(is_zoo || is_shell || is_clear || removed_roles.contains(&entity)) {
            continue;
        }
        let active = if is_zoo {
            zoo == Some(entity)
        } else if is_shell {
            zoo.is_none() && shell == Some(entity)
        } else {
            is_clear && zoo.is_none()
        };
        camera
            .map_unchanged(|camera| &mut camera.is_active)
            .set_if_neq(active);
        match (is_default, target == Some(entity)) {
            (true, false) => {
                commands.entity(entity).remove::<IsDefaultUiCamera>();
            }
            (false, true) => {
                commands.entity(entity).insert(IsDefaultUiCamera);
            }
            _ => {}
        }
    }
}
