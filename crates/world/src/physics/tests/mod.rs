use avian3d::prelude::*;
use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::{
    animation_graph::animation_graph_playback_message_types::{
        AnimationClipActionOptions, AnimationClipPlaybackRequest,
    },
    animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy,
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::animal_behavior::behavior_world_release_types::{
    BehaviorWorldObjectReleased, BehaviorWorldReleaseMotion, FallReleaseAnimationClips,
};

use crate::physics::interaction_execution::apply_requested_impulses_to_dynamic_avian_bodies;
use crate::physics::interaction_execution::{
    fall_landing_surface, release_behavior_object_into_world,
};
use crate::physics::interaction_types::ApplyPhysicsImpulseRequest;
use crate::physics::interaction_types::PhysicsInteractionFailure;

fn create_physics_test_application() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        PhysicsPlugins::default(),
    ))
    .init_asset::<Mesh>()
    .add_message::<ApplyPhysicsImpulseRequest>()
    .add_message::<PhysicsInteractionFailure>()
    .add_systems(
        FixedPostUpdate,
        apply_requested_impulses_to_dynamic_avian_bodies,
    )
    .insert_resource(TimeUpdateStrategy::FixedTimesteps(1));
    app.finish();
    app.cleanup();
    app
}

#[test]
fn impulse_changes_only_a_dynamic_avian_body() {
    let mut app = create_physics_test_application();
    let dynamic = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            Transform::IDENTITY,
        ))
        .id();
    app.update();
    app.world_mut().write_message(ApplyPhysicsImpulseRequest {
        entity: dynamic,
        impulse_ns: Vec3::X,
        point_world: Vec3::ZERO,
    });
    app.update();
    assert!(app.world().get::<LinearVelocity>(dynamic).unwrap().x > 0.0);
}

#[test]
fn collision_events_are_opt_in_at_the_collider() {
    let mut app = create_physics_test_application();
    let ground = app
        .world_mut()
        .spawn((
            RigidBody::Static,
            Collider::cuboid(4.0, 0.2, 4.0),
            Transform::IDENTITY,
            CollisionEventsEnabled,
        ))
        .id();
    let body = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::sphere(0.5),
            Transform::from_xyz(0.0, 0.25, 0.0),
            CollisionEventsEnabled,
        ))
        .id();
    let mut cursor = app
        .world()
        .resource::<Messages<CollisionStart>>()
        .get_cursor();
    app.update();
    app.update();
    let collisions = app.world().resource::<Messages<CollisionStart>>();
    assert!(cursor.read(collisions).any(|collision| {
        collision.collider1 == ground && collision.collider2 == body
            || collision.collider1 == body && collision.collider2 == ground
    }));
}

#[test]
fn fall_uses_the_first_eligible_support_kind() {
    assert_eq!(
        fall_landing_surface(2.0, 0.0, -1.0, Some(1.5), Some(1.0)),
        Some((1.0, false))
    );
    assert_eq!(
        fall_landing_surface(2.0, 0.0, 0.5, Some(1.5), None),
        Some((1.5, true))
    );
    assert_eq!(
        fall_landing_surface(2.0, 0.0, 1.5, None, Some(2.0)),
        Some((1.5, false))
    );
    assert_eq!(
        fall_landing_surface(2.0, 0.0, 1.5, Some(1.5), None),
        Some((1.5, true))
    );
    assert_eq!(fall_landing_surface(2.0, 1.0, 0.0, Some(1.0), None), None);
}

#[test]
fn fall_release_requests_the_authored_airborne_loop() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<WorldDefinitionAsset>()
        .init_resource::<WorldDefinitions>()
        .add_message::<AnimationClipPlaybackRequest>()
        .add_message::<AnimationClipActionOptions>()
        .add_observer(release_behavior_object_into_world);
    let world = app.world_mut();
    let root = world.spawn_empty().id();
    let object = world
        .spawn((
            WorldMember { root },
            Transform::IDENTITY,
            FallReleaseAnimationClips {
                airborne: Some("CustomFall".to_owned()),
                ..default()
            },
        ))
        .id();
    world.trigger(BehaviorWorldObjectReleased {
        object,
        world: root,
        pose: Transform::from_translation(Vec3::Y),
        motion: BehaviorWorldReleaseMotion::Fall,
    });
    let requests = world.resource::<Messages<AnimationClipPlaybackRequest>>();
    let mut cursor = requests.get_cursor();
    let mut requests = cursor.read(requests);
    let request = requests.next().unwrap();
    assert_eq!(request.animation_subject_entity, object);
    assert_eq!(request.animation_clip_asset_key, "CustomFall");
    assert_eq!(
        request.playback_repetition_policy,
        AnimationPlaybackRepetitionPolicy::Loop
    );
    assert!(requests.next().is_none());
}
