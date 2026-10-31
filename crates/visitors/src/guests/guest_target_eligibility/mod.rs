//! Authored distance and trash-level qualifiers for concrete guest destination
//! candidates.

use bevy::prelude::*;
use openzt2_game_data::behavior::eligibility::{
    BehaviorCandidateEligibilityFact, BehaviorConditionEligibilityFact,
    BehaviorEligibilityFactComparison, BehaviorEligibilityFactInput, BehaviorEligibilityFactValue,
    BehaviorSpatialEligibilityFact,
};

pub(super) fn satisfies_guest_target_fact(
    fact: &BehaviorCandidateEligibilityFact,
    actor_position: Vec3,
    target_position: Vec3,
    target_elevated_path: Option<bool>,
    target_trash_level: Option<u16>,
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
        // A waste container's `f_TrashLevel` is its contained waste, such as
        // the `L 100` a guest's trash can must not yet have reached.
        BehaviorEligibilityFactInput::Condition(BehaviorConditionEligibilityFact::TrashLevel) => {
            let Some(level) = target_trash_level else {
                return false;
            };
            compare(i32::from(level) << 16, fact.comparison, expected)
        }
        BehaviorEligibilityFactInput::Spatial(
            kind @ (BehaviorSpatialEligibilityFact::SupportedSurface
            | BehaviorSpatialEligibilityFact::ElevatedPath),
        ) => {
            let actual = if kind == BehaviorSpatialEligibilityFact::ElevatedPath {
                // Only path targets know whether they are elevated.
                let Some(elevated) = target_elevated_path else {
                    return false;
                };
                i32::from(elevated) << 16
            } else {
                1 << 16
            };
            compare(actual, fact.comparison, expected)
        }
        _ => false,
    }
}

fn compare(actual: i32, comparison: BehaviorEligibilityFactComparison, expected: i32) -> bool {
    match comparison {
        BehaviorEligibilityFactComparison::Equal => actual == expected,
        BehaviorEligibilityFactComparison::NotEqual => actual != expected,
        BehaviorEligibilityFactComparison::Less => actual < expected,
        BehaviorEligibilityFactComparison::LessOrEqual => actual <= expected,
        BehaviorEligibilityFactComparison::Greater => actual > expected,
        BehaviorEligibilityFactComparison::GreaterOrEqual => actual >= expected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_trash_can_qualifier_reads_the_contained_waste() {
        // Trash.tsk ThrowAwayTrashCan: `f_TrashLevel="L 100"`.
        let below_full = BehaviorCandidateEligibilityFact {
            fact_input: BehaviorEligibilityFactInput::Condition(
                BehaviorConditionEligibilityFact::TrashLevel,
            ),
            comparison: BehaviorEligibilityFactComparison::Less,
            expected_value: BehaviorEligibilityFactValue::Q16(100 << 16),
        };
        let check =
            |level| satisfies_guest_target_fact(&below_full, Vec3::ZERO, Vec3::X, None, level);
        assert!(check(Some(0)));
        assert!(check(Some(99)));
        assert!(!check(Some(100)));
        assert!(!check(None));
    }
}
