//! Authored distance qualifiers for concrete guest destination candidates.

use bevy::prelude::*;
use openzt2_game_data::behavior::eligibility::{
    BehaviorCandidateEligibilityFact, BehaviorEligibilityFactComparison,
    BehaviorEligibilityFactInput, BehaviorEligibilityFactValue, BehaviorSpatialEligibilityFact,
};

pub(super) fn satisfies_guest_target_fact(
    fact: &BehaviorCandidateEligibilityFact,
    actor_position: Vec3,
    target_position: Vec3,
) -> bool {
    let BehaviorEligibilityFactValue::Q16(expected) = fact.expected_value else {
        return false;
    };
    match fact.fact_input {
        BehaviorEligibilityFactInput::Spatial(
            kind @ (BehaviorSpatialEligibilityFact::InSight
            | BehaviorSpatialEligibilityFact::NotInSight),
        ) if fact.comparison == BehaviorEligibilityFactComparison::Less => {
            // The planar distance must be strictly less than the radius.
            // This qualifier does not perform an obstruction/visibility raycast.
            let distance_squared = actor_position.xz().distance_squared(target_position.xz());
            if !distance_squared.is_finite() {
                return false;
            }
            let radius = expected as f32 / 65_536.0;
            let inside = distance_squared < radius * radius;
            if kind == BehaviorSpatialEligibilityFact::NotInSight {
                !inside
            } else {
                inside
            }
        }
        BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::SupportedSurface) => {
            let actual = 1 << 16;
            match fact.comparison {
                BehaviorEligibilityFactComparison::Equal => actual == expected,
                BehaviorEligibilityFactComparison::NotEqual => actual != expected,
                BehaviorEligibilityFactComparison::Less => actual < expected,
                BehaviorEligibilityFactComparison::LessOrEqual => actual <= expected,
                BehaviorEligibilityFactComparison::Greater => actual > expected,
                BehaviorEligibilityFactComparison::GreaterOrEqual => actual >= expected,
            }
        }
        _ => false,
    }
}
