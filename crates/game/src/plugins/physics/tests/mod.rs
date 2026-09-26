use avian3d::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::{
    asset::{AssetApp, AssetPlugin},
    prelude::*,
};

use super::{
    interaction_execution::{
        apply_requested_impulses_to_dynamic_avian_bodies,
        publish_gameplay_contact_facts_from_avian_collision_events,
    },
    interaction_types::{
        ApplyPhysicsImpulseRequest, PhysicsContactFact, PhysicsContactFactKind,
        PhysicsInteractionFailure,
    },
};

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
    .add_message::<PhysicsContactFact>()
    .add_message::<PhysicsInteractionFailure>()
    .add_message::<CollisionStart>()
    .add_message::<CollisionEnd>()
    .add_systems(
        FixedPostUpdate,
        apply_requested_impulses_to_dynamic_avian_bodies,
    )
    .add_systems(
        FixedPostUpdate,
        publish_gameplay_contact_facts_from_avian_collision_events.after(PhysicsSystems::Writeback),
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
        .resource::<Messages<PhysicsContactFact>>()
        .get_cursor();
    app.update();
    app.update();
    let facts = app.world().resource::<Messages<PhysicsContactFact>>();
    assert!(cursor.read(facts).any(|fact| {
        (fact.a == ground && fact.b == body || fact.a == body && fact.b == ground)
            && fact.kind == PhysicsContactFactKind::Enter
    }));
}
