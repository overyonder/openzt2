use bevy::prelude::*;

pub(super) fn is_fossil_site_within_authored_dig_area(
    camera: &GlobalTransform,
    site: Vec3,
    dig_distance_m: f32,
) -> bool {
    if !dig_distance_m.is_finite() || dig_distance_m <= 0.0 || !site.is_finite() {
        return false;
    }
    // Preserve camera pitch: native projection offsets by the full camera
    // direction, then measures horizontal distance around that projected point.
    let center = camera.translation() + camera.forward().as_vec3() * dig_distance_m;
    center.is_finite() && center.xz().distance_squared(site.xz()) <= dig_distance_m.powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fossil_dig_area_uses_camera_projection_and_an_inclusive_horizontal_radius() {
        let camera = GlobalTransform::IDENTITY;
        let radius = 1.75;
        assert!(is_fossil_site_within_authored_dig_area(
            &camera,
            Vec3::new(0.0, 30.0, -3.5),
            radius
        ));
        assert!(!is_fossil_site_within_authored_dig_area(
            &camera,
            Vec3::new(0.0, 0.0, -3.51),
            radius
        ));
        assert!(!is_fossil_site_within_authored_dig_area(
            &camera,
            Vec3::new(0.0, 0.0, 0.1),
            radius
        ));
        let moved = GlobalTransform::from_translation(Vec3::X * 10.0);
        assert!(!is_fossil_site_within_authored_dig_area(
            &moved,
            Vec3::new(0.0, 0.0, -1.75),
            radius
        ));
        let downward = GlobalTransform::from(Transform::from_rotation(Quat::from_rotation_x(
            -std::f32::consts::FRAC_PI_2,
        )));
        assert!(!is_fossil_site_within_authored_dig_area(
            &downward,
            Vec3::new(0.0, 0.0, -3.0),
            radius
        ));
    }
}
