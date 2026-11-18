use bevy::{
    pbr::{DistanceFog, FogFalloff},
    prelude::*,
};
use openzt2_zoo_state::environment::environment_state_types::WorldEnvironment;

use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;

use super::environment_presentation_types::{AboveWaterFog, EnvironmentFog};

pub(super) fn attach_neutral_environment_fog_to_new_zoo_cameras(
    mut commands: Commands,
    environments: Query<Entity, With<WorldEnvironment>>,
    mut cameras: Query<
        (Entity, Option<&mut DistanceFog>),
        (With<ZooCamera>, Without<EnvironmentFog>),
    >,
) {
    let Ok(environment) = environments.single() else {
        return;
    };
    let neutral = AboveWaterFog::default();
    for (camera, fog) in &mut cameras {
        let sample = DistanceFog {
            falloff: FogFalloff::Linear {
                start: neutral.start,
                end: neutral.end,
            },
            ..default()
        };
        if let Some(mut fog) = fog {
            *fog = sample;
        } else {
            commands.entity(camera).insert(sample);
        }
        commands.entity(camera).insert(EnvironmentFog(environment));
    }
}

pub(super) fn remove_camera_fog_when_environment_is_unlinked(
    removed: On<Remove<EnvironmentFog>>,
    mut commands: Commands,
) {
    commands.entity(removed.entity).try_remove::<DistanceFog>();
}
