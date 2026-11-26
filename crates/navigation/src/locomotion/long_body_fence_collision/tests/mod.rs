use super::wall_push;
use bevy::prelude::*;

#[test]
fn a_long_body_is_pushed_off_a_wall_it_reaches_into() {
    // A 4 m body lying beside a wall along x, its tail end 0.3 m from the
    // wall's middle: radius 0.6 and half thickness 0.125 need 0.725.
    let wall = (Vec2::new(-5.0, 0.0), Vec2::new(5.0, 0.0));
    let push = wall_push(
        Vec2::new(0.0, 0.3),
        Vec2::new(0.0, 4.3),
        0.6,
        wall.0,
        wall.1,
        0.125,
        Vec2::new(0.0, 2.3),
    );
    assert!((push - Vec2::new(0.0, 0.425)).length() < 1.0e-4, "{push}");
    // Clear of the wall, nothing moves.
    assert_eq!(
        wall_push(
            Vec2::new(0.0, 1.0),
            Vec2::new(0.0, 5.0),
            0.6,
            wall.0,
            wall.1,
            0.125,
            Vec2::new(0.0, 3.0)
        ),
        Vec2::ZERO
    );
    // A body lying across the wall goes back to its centre's side.
    let push = wall_push(
        Vec2::new(0.0, -1.0),
        Vec2::new(0.0, 3.0),
        0.6,
        wall.0,
        wall.1,
        0.125,
        Vec2::new(0.0, 1.0),
    );
    assert!(push.y > 0.0 && push.x.abs() < 1.0e-4, "{push}");
}

#[test]
fn two_bodies_side_by_side_move_apart_by_half_their_overlap_each() {
    // Two 4 m bodies of radius 0.6 lying 1 m apart overlap by 0.2 m.
    let left = [(
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 4.0),
        0.6,
        Vec2::new(0.0, 2.0),
    )];
    let right = [(
        Vec2::new(1.0, 1.0),
        Vec2::new(1.0, 5.0),
        0.6,
        Vec2::new(1.0, 3.0),
    )];
    let far = [(
        Vec2::new(9.0, 0.0),
        Vec2::new(9.0, 4.0),
        0.6,
        Vec2::new(9.0, 2.0),
    )];
    let pushes = super::body_pushes([&left[..], &right[..], &far[..]].into_iter());
    assert!(
        (pushes[0] - Vec2::new(-0.1, 0.0)).length() < 1.0e-4,
        "{pushes:?}"
    );
    assert!(
        (pushes[1] - Vec2::new(0.1, 0.0)).length() < 1.0e-4,
        "{pushes:?}"
    );
    assert_eq!(pushes[2], Vec2::ZERO);
}
