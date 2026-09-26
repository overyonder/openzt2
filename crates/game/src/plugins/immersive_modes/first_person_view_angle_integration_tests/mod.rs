use bevy::prelude::*;

use super::{
    first_person_view_angle_integration::integrate_first_person_view_angles_from_look_input_with_authored_speed_and_bounds,
    immersive_mode_control_types::FirstPersonViewAngles,
};

#[test]
fn first_person_view_angles_use_authored_speed_and_bounds() {
    let mut view_angles = FirstPersonViewAngles::default();
    integrate_first_person_view_angles_from_look_input_with_authored_speed_and_bounds(
        &mut view_angles,
        Vec2::new(1.0, -1.0),
        0.0,
        2.0,
        &(-0.5..=0.5),
        &(-1.0..=1.0),
        1.0,
    );
    assert_eq!(view_angles.pitch, 0.5);
    assert_eq!(view_angles.yaw, -1.0);
    let camera_forward = Quat::from_rotation_y(view_angles.yaw) * Vec3::NEG_Z;
    assert!(camera_forward.x > 0.0, "right input must look toward +X");
}
