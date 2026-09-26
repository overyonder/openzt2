use bevy::prelude::*;

use super::transport_fact_validation::transport_track_grade_is_within_authored_limit;

#[test]
fn authored_track_grade_limit_is_enforced_from_live_endpoint_positions() {
    assert!(transport_track_grade_is_within_authored_limit(
        Vec3::ZERO,
        Vec3::new(10.0, 1.0, 0.0),
        100,
    ));
    assert!(!transport_track_grade_is_within_authored_limit(
        Vec3::ZERO,
        Vec3::new(10.0, 1.01, 0.0),
        100,
    ));
    assert!(!transport_track_grade_is_within_authored_limit(
        Vec3::ZERO,
        Vec3::new(0.0, 1.0, 0.0),
        1000,
    ));
}
