use bevy::prelude::*;

pub(super) fn camera_pose_materially_changed(current: &Transform, next: &Transform) -> bool {
    current.translation.distance_squared(next.translation) > 1.0e-8
        // acos(dot) amplifies normalization roundoff close to zero, so even
        // identical look-at quaternions can report a nonzero angle. Compare
        // components directly, accounting for q and -q being one rotation.
        || !(current.rotation.abs_diff_eq(next.rotation, 5.0e-6)
            || current.rotation.abs_diff_eq(-next.rotation, 5.0e-6))
        || current.scale.distance_squared(next.scale) > 1.0e-8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_and_sign_reversed_look_at_rotations_do_not_invalidate_camera_pose() {
        for yaw in [-3.0_f32, -0.651_134_97, 0.0, 0.7, 3.0] {
            let eye = Vec3::new(-yaw.sin() * 20.0, 64.837_05, yaw.cos() * 20.0);
            let pose = Transform::from_translation(eye).looking_at(Vec3::ZERO, Vec3::Y);
            assert!(!camera_pose_materially_changed(&pose, &pose));
            let mut equivalent = pose;
            equivalent.rotation = -pose.rotation;
            assert!(!camera_pose_materially_changed(&pose, &equivalent));
            let mut moved = pose;
            moved.translation.x += 0.001;
            assert!(camera_pose_materially_changed(&pose, &moved));
            let mut turned = pose;
            turned.rotate_y(0.001);
            assert!(camera_pose_materially_changed(&pose, &turned));
        }
    }
}
