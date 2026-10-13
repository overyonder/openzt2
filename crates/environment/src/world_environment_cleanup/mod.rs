use bevy::{
    ecs::{entity::EntityHashSet, entity_disabling::Disabled, query::Allow},
    pbr::DistanceFog,
    prelude::*,
};
use openzt2_zoo_state::environment::environment_presentation_types::EnvironmentLight;
use openzt2_zoo_state::environment::environment_state_types::WorldEnvironment;

use openzt2_world_state::world_spawn::world_unloading::queue_owned_hierarchy_roots_for_retirement;

use super::{
    ambient_animal_types::{AmbientAnimal, AmbientAnimalSpawner},
    environment_presentation_types::{
        EnvironmentFog, EnvironmentModelVisual, EnvironmentSky, EnvironmentTexture,
    },
};

pub(super) fn remove_world_environment_entities_and_camera_fog_after_leaving_game(
    mut commands: Commands,
    environment_owned_entities: Query<
        Entity,
        (
            Or<(
                With<WorldEnvironment>,
                With<EnvironmentLight>,
                With<EnvironmentSky>,
                With<EnvironmentTexture>,
                With<EnvironmentModelVisual>,
                With<AmbientAnimalSpawner>,
                With<AmbientAnimal>,
            )>,
            Allow<Disabled>,
        ),
    >,
    cameras: Query<Entity, (With<EnvironmentFog>, Allow<Disabled>)>,
    parents: Query<&ChildOf, Allow<Disabled>>,
) {
    let owned_entities: EntityHashSet = environment_owned_entities.iter().collect();
    queue_owned_hierarchy_roots_for_retirement(&mut commands, &owned_entities, &parents);
    for camera in &cameras {
        if !owned_entities.contains(&camera)
            && !parents
                .iter_ancestors::<ChildOf>(camera)
                .any(|ancestor| owned_entities.contains(&ancestor))
        {
            commands
                .entity(camera)
                .remove::<(EnvironmentFog, DistanceFog)>();
        }
    }
}
