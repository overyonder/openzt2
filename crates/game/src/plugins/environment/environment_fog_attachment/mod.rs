use bevy::{
    pbr::{DistanceFog, FogFalloff},
    prelude::*,
};

use crate::plugins::camera::camera_runtime_state_types::ZooCamera;

use super::{
    environment_presentation_types::{EnvironmentFog, EnvironmentPresentationPending},
    environment_state_types::WorldEnvironment,
};

pub(super) fn attach_neutral_environment_fog_to_new_zoo_cameras(
    mut commands: Commands,
    environments: Query<Entity, With<WorldEnvironment>>,
    cameras: Query<Entity, (With<ZooCamera>, Added<ZooCamera>)>,
) {
    if environments.is_empty() {
        return;
    }
    let mut attached_to_new_camera = false;
    for camera in &cameras {
        insert_neutral_environment_fog(&mut commands, camera);
        attached_to_new_camera = true;
    }
    if attached_to_new_camera {
        for environment in &environments {
            commands
                .entity(environment)
                .insert(EnvironmentPresentationPending);
        }
    }
}

pub(super) fn insert_neutral_environment_fog(commands: &mut Commands, camera: Entity) {
    commands.entity(camera).insert((
        EnvironmentFog,
        DistanceFog {
            // Daylight presentation replaces this neutral distance as soon as
            // the camera and environment coexist. Bevy's 0..1 metre default
            // would otherwise hide the zoo for one frame.
            falloff: FogFalloff::Linear {
                start: 1_000_000.0,
                end: 1_000_001.0,
            },
            ..default()
        },
    ));
}
