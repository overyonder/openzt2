use bevy::prelude::*;

use crate::plugins::world_spawn::{
    world_membership_types::{WorldMember, WorldRoot},
    world_unloading::assert_no_invalid_entity_commands_during,
    WorldSpawnPlugin,
};
use openzt2_game_assets::application_lifecycle::GamePhase;

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
fn registered_world_exit_flushes_members_before_environment_and_surviving_camera_cleanup() {
    use bevy::ecs::entity_disabling::Disabled;

    use super::{environment_presentation_types::EnvironmentModelVisual, EnvironmentPlugin};

    let mut application = App::new();
    application
        .add_plugins((WorldSpawnPlugin, EnvironmentPlugin))
        .edit_schedule(OnExit(GamePhase::InGame), |schedule| {
            schedule.set_executor(bevy::ecs::schedule::SingleThreadedExecutor::new());
        });
    let root = application
        .world_mut()
        .spawn(WorldRoot {
            scenario: asset_identifier_with_first_byte(1),
        })
        .id();
    let environment = application
        .world_mut()
        .spawn((
            WorldEnvironment {
                definition: asset_identifier_with_first_byte(2),
            },
            WorldMember { root },
            Disabled,
        ))
        .id();
    let visual = application
        .world_mut()
        .spawn((
            EnvironmentModelVisual,
            WorldMember { root },
            ChildOf(environment),
        ))
        .id();
    let renderer = application.world_mut().spawn(ChildOf(visual)).id();
    let world_camera = application
        .world_mut()
        .spawn((
            WorldMember { root },
            EnvironmentFog,
            bevy::pbr::DistanceFog::default(),
        ))
        .id();
    let orphan_member = application
        .world_mut()
        .spawn((
            WorldMember {
                root: Entity::PLACEHOLDER,
            },
            Disabled,
        ))
        .id();
    let unrelated_parent = application.world_mut().spawn_empty().id();
    let environment_without_membership = application
        .world_mut()
        .spawn((
            WorldEnvironment {
                definition: asset_identifier_with_first_byte(3),
            },
            ChildOf(unrelated_parent),
            Disabled,
        ))
        .id();
    let leftover_visual = application
        .world_mut()
        .spawn((
            EnvironmentModelVisual,
            ChildOf(environment_without_membership),
        ))
        .id();
    let environment_camera = application
        .world_mut()
        .spawn((
            EnvironmentFog,
            bevy::pbr::DistanceFog::default(),
            ChildOf(leftover_visual),
            Disabled,
        ))
        .id();
    let surviving_camera = application
        .world_mut()
        .spawn((
            EnvironmentFog,
            bevy::pbr::DistanceFog::default(),
            ChildOf(unrelated_parent),
            Disabled,
        ))
        .id();

    // Run the registered exit boundary without running unrelated loading or
    // presentation systems, which require a complete asset environment.
    assert_no_invalid_entity_commands_during(|| {
        application
            .world_mut()
            .run_schedule(OnExit(GamePhase::InGame));
    });
    for entity in [
        root,
        environment,
        visual,
        renderer,
        world_camera,
        orphan_member,
        environment_without_membership,
        leftover_visual,
        environment_camera,
    ] {
        assert!(
            application.world().get_entity(entity).is_err(),
            "retained {entity}"
        );
    }
    assert!(application.world().get_entity(unrelated_parent).is_ok());
    assert!(application.world().get_entity(surviving_camera).is_ok());
    assert!(application
        .world()
        .get::<EnvironmentFog>(surviving_camera)
        .is_none());
    assert!(application
        .world()
        .get::<bevy::pbr::DistanceFog>(surviving_camera)
        .is_none());
    assert_no_invalid_entity_commands_during(|| {
        application
            .world_mut()
            .run_schedule(OnExit(GamePhase::InGame));
    });
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
