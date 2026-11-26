use super::staff_request_fact_changes::StaffRequestFactValue;
use openzt2_game_data::world_definitions::staff_management::{
    StaffRequestThresholdComparison as Comparison, StaffRequestThresholdValue,
};

pub(super) fn staff_threshold_matches(
    comparison: Comparison,
    threshold: StaffRequestThresholdValue,
    sample: StaffRequestFactValue,
) -> bool {
    match (threshold, sample) {
        (StaffRequestThresholdValue::Number(value), StaffRequestFactValue::Number(current))
            if value.is_finite() && current.is_finite() =>
        {
            match comparison {
                Comparison::EntersEquality => current == value,
                Comparison::FallsBelow => current < value,
                Comparison::RisesAbove => current > value,
                Comparison::ReachesOrBelow => current <= value,
                Comparison::ReachesOrAbove => current >= value,
            }
        }
        (StaffRequestThresholdValue::Boolean(threshold), StaffRequestFactValue::Boolean(value)) => {
            value == threshold
        }
        _ => false,
    }
}
