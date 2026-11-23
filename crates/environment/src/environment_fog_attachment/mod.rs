use bevy::{
    ecs::relationship::RelationshipTarget,
    pbr::{DistanceFog, FogFalloff},
    prelude::*,
};
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::environment_presentation_types::{AboveWaterFog, EnvironmentFog, WorldEnvironments};

pub(super) fn attach_neutral_environment_fog_to_new_zoo_cameras(
    mut commands: Commands,
    environments: Query<&WorldEnvironments>,
    mut cameras: Query<
        (Entity, &WorldMember, Option<&mut DistanceFog>),
        (With<ZooCamera>, Without<EnvironmentFog>),
    >,
) {
    let neutral = AboveWaterFog::default();
    for (camera, member, fog) in &mut cameras {
        let Some(environment) = environments
            .get(member.root)
            .ok()
            .and_then(|environments| environments.iter().next())
        else {
            continue;
        };
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
