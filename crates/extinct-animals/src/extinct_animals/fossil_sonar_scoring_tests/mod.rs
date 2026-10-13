use bevy::prelude::{Entity, Vec3};

use super::fossil_sonar_scoring::select_highest_scoring_fossil_sonar_candidate;

#[test]
fn fossil_sonar_scores_in_one_pass_and_preserves_first_tie() {
    let first_candidate = Entity::from_bits(1);
    let tied_candidate = Entity::from_bits(2);
    let maximum_distance_candidate = Entity::from_bits(3);
    let result = select_highest_scoring_fossil_sonar_candidate(
        Vec3::ZERO,
        Vec3::Z,
        None,
        36.0,
        576.0,
        std::f32::consts::FRAC_1_SQRT_2,
        [
            (first_candidate, Vec3::new(0.0, 0.0, 12.0)),
            (tied_candidate, Vec3::new(0.0, 4.0, 12.0)),
            (maximum_distance_candidate, Vec3::new(0.0, 0.0, 24.0)),
        ],
    )
    .expect("one in-range candidate");
    assert_eq!(result.entity, first_candidate);
    assert_eq!(result.distance_squared, 144.0);
    assert!((result.score - 0.8).abs() < 1e-6);
}

#[test]
fn fossil_sonar_omits_retained_and_out_of_cone_artifacts() {
    let retained_artifact = Entity::from_bits(1);
    let behind_camera_artifact = Entity::from_bits(2);
    assert!(select_highest_scoring_fossil_sonar_candidate(
        Vec3::ZERO,
        Vec3::Z,
        Some(retained_artifact),
        36.0,
        576.0,
        std::f32::consts::FRAC_1_SQRT_2,
        [
            (retained_artifact, Vec3::new(0.0, 0.0, 10.0)),
            (behind_camera_artifact, Vec3::new(0.0, 0.0, -10.0)),
        ],
    )
    .is_none());
}
