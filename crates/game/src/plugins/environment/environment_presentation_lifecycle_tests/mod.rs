use bevy::prelude::*;

use super::{
    ambient_animal_types::AmbientAnimal,
    environment_fog_attachment::attach_neutral_environment_fog_to_new_zoo_cameras,
    environment_presentation_types::{EnvironmentFog, EnvironmentPresentationPending},
    environment_state_types::WorldEnvironment,
    world_environment_cleanup::remove_world_environment_entities_and_camera_fog_after_leaving_game,
};

fn asset_identifier_with_first_byte(first_byte: u8) -> openzt2_game_data::AssetId {
    let mut bytes = [0; 16];
    bytes[0] = first_byte;
    openzt2_game_data::AssetId(bytes)
}

#[test]
fn cleanup_removes_owned_entities_and_detaches_fog_from_camera() {
    let mut application = App::new();
    application.add_systems(
        Update,
        remove_world_environment_entities_and_camera_fog_after_leaving_game,
    );
    let environment = application
        .world_mut()
        .spawn(WorldEnvironment {
            definition: asset_identifier_with_first_byte(1),
        })
        .id();
    let ambient_animal = application
        .world_mut()
        .spawn(AmbientAnimal {
            definition: asset_identifier_with_first_byte(2),
            despawn_tick: 10,
        })
        .id();
    let camera = application
        .world_mut()
        .spawn((EnvironmentFog, bevy::pbr::DistanceFog::default()))
        .id();
    application.update();

    assert!(application.world().get_entity(environment).is_err());
    assert!(application.world().get_entity(ambient_animal).is_err());
    assert!(application.world().get::<EnvironmentFog>(camera).is_none());
    assert!(application
        .world()
        .get::<bevy::pbr::DistanceFog>(camera)
        .is_none());
}

#[test]
fn camera_added_after_environment_receives_renderer_owned_fog() {
    let mut application = App::new();
    application.add_systems(Update, attach_neutral_environment_fog_to_new_zoo_cameras);
    let environment = application
        .world_mut()
        .spawn(WorldEnvironment {
            definition: asset_identifier_with_first_byte(1),
        })
        .id();
    let camera = application
        .world_mut()
        .spawn(crate::plugins::camera::camera_runtime_state_types::ZooCamera)
        .id();
    application.update();

    assert!(application.world().get::<EnvironmentFog>(camera).is_some());
    assert!(application
        .world()
        .get::<bevy::pbr::DistanceFog>(camera)
        .is_some());
    assert!(application
        .world()
        .get::<EnvironmentPresentationPending>(environment)
        .is_some());
}
