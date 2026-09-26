use bevy::prelude::Vec2;

use super::locomotion_types::{DirectLocomotion, NavAgent, NavFlags, Route, SpatialGrid};

#[test]
fn physics_navigation_faces_authored_forward_and_stays_stopped_after_arrival() {
    use super::locomotion_types::{Arrived, Destination, Steering};
    use avian3d::prelude::{AngularVelocity, LinearVelocity, RigidBody};
    use bevy::prelude::*;

    let mut app = App::new();
    let mut time = Time::<Fixed>::default();
    time.advance_by(std::time::Duration::from_millis(20));
    app.insert_resource(time).add_message::<Arrived>().add_systems(
        Update,
        super::avian_navigation_agent_driving::drive_avian_navigation_agent_velocities_and_complete_routes,
    );
    let agent = app
        .world_mut()
        .spawn((
            NavAgent {
                radius_m: 0.3,
                max_speed_mps: 2.0,
                acceleration_mps2: 4.0,
                capabilities: NavFlags::GUEST,
            },
            GlobalTransform::IDENTITY,
            RigidBody::Kinematic,
            LinearVelocity::default(),
            AngularVelocity::default(),
            Steering {
                desired_velocity: Vec3::Z,
            },
            Destination {
                request_id: 1,
                world: Vec3::Z * 10.0,
                arrival_radius_m: 0.3,
            },
        ))
        .id();
    app.update();
    assert!(app
        .world()
        .get::<AngularVelocity>(agent)
        .is_some_and(|velocity| velocity.y.abs() < 1.0e-6));
    // Completing a route clears Destination. Retained steering is deliberately
    // nonzero, as it is no longer visited by the route-following query.
    app.world_mut().entity_mut(agent).remove::<Destination>();
    app.update();
    app.update();
    assert!(app
        .world()
        .get::<LinearVelocity>(agent)
        .is_some_and(|velocity| velocity.length_squared() < 1.0e-10));
    app.world_mut().entity_mut(agent).insert(DirectLocomotion {
        local_axes: Vec2::Y,
    });
    app.update();
    assert!(app
        .world()
        .get::<LinearVelocity>(agent)
        .is_some_and(|velocity| velocity.z > 0.0));
    app.world_mut().entity_mut(agent).insert(Steering {
        desired_velocity: Vec3::X,
    });
    app.update();
    assert!(app
        .world()
        .get::<AngularVelocity>(agent)
        .is_some_and(|velocity| velocity.y == 0.0));
}

#[test]
fn docking_finishes_in_world_coordinates_under_a_transformed_parent() {
    use super::locomotion_types::{Arrived, Destination, Docking, NavigationFailed, Steering};
    use bevy::prelude::*;

    let mut app = App::new();
    app.add_message::<Arrived>()
        .add_message::<NavigationFailed>()
        .add_systems(
            Update,
            super::spatial_index_and_docking_operations::complete_docking,
        );
    let parent_transform = GlobalTransform::from(Transform {
        translation: Vec3::new(10.0, 2.0, -5.0),
        rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        scale: Vec3::splat(2.0),
    });
    let parent = app.world_mut().spawn(parent_transform).id();
    let target = app.world_mut().spawn(GlobalTransform::IDENTITY).id();
    let point = parent_transform.transform_point(Vec3::Z);
    let agent = app
        .world_mut()
        .spawn((
            Transform::from_translation(Vec3::Z),
            ChildOf(parent),
            Docking {
                request_id: 1,
                target,
                point,
                forward: Vec3::Z,
                radius_m: 0.1,
            },
            Destination {
                request_id: 1,
                world: point,
                arrival_radius_m: 0.1,
            },
            Route {
                points: vec![point],
                cursor: 0,
            },
            Steering {
                desired_velocity: Vec3::X,
            },
        ))
        .id();
    app.update();
    assert!(app.world().get::<Docking>(agent).is_none());
    assert!(app.world().get::<Transform>(agent).is_some_and(|local| {
        parent_transform
            .transform_point(local.translation)
            .abs_diff_eq(point, 1.0e-5)
            && (parent_transform.rotation() * local.rotation * Vec3::Z).abs_diff_eq(Vec3::Z, 1.0e-5)
    }));
    assert!(app
        .world()
        .get::<Route>(agent)
        .is_some_and(|route| route.points.is_empty()));
    assert!(app
        .world()
        .get::<Steering>(agent)
        .is_some_and(|steering| steering.desired_velocity == Vec3::ZERO));
    assert_eq!(app.world().resource::<Messages<Arrived>>().len(), 1);
}

#[test]
fn cell_mapping_rejects_world_exterior() {
    let mut grid = SpatialGrid::default();
    assert!(grid.reserve_layout(Vec2::new(-10.0, -10.0), 5.0, 4, 4, 0));
    assert_eq!(grid.cell_of(Vec2::new(-9.0, -9.0)), Some(0));
    assert_eq!(grid.cell_of(Vec2::new(9.9, 9.9)), Some(15));
    assert_eq!(grid.cell_of(Vec2::new(10.0, 0.0)), None);
}
