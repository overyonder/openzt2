use bevy::{pbr::DistanceFog, prelude::*};

use super::{
    ambient_animal_types::{AmbientAnimal, AmbientAnimalSpawner},
    environment_presentation_types::{
        EnvironmentFog, EnvironmentLight, EnvironmentModelVisual, EnvironmentSky,
        EnvironmentTexture,
    },
    environment_state_types::WorldEnvironment,
};

pub(super) fn remove_world_environment_entities_and_camera_fog_after_leaving_game(
    mut commands: Commands,
    environment_owned_entities: Query<
        Entity,
        Or<(
            With<WorldEnvironment>,
            With<EnvironmentLight>,
            With<EnvironmentSky>,
            With<EnvironmentTexture>,
            With<EnvironmentModelVisual>,
            With<AmbientAnimalSpawner>,
            With<AmbientAnimal>,
        )>,
    >,
    mut cameras: Query<Entity, With<EnvironmentFog>>,
) {
    for entity in &environment_owned_entities {
        commands.entity(entity).despawn();
    }
    for camera in &mut cameras {
        commands
            .entity(camera)
            .remove::<EnvironmentFog>()
            .remove::<DistanceFog>();
    }
}
