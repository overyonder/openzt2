use super::staff_request_state_types::StaffRequestControllerSample;
use openzt2_game_data::world_definitions::staff_management::StaffRequestThresholdComparison;
use openzt2_game_data::world_definitions::staff_management::StaffRequestThresholdValue;

pub(super) fn staff_threshold_matches(
    comparison: StaffRequestThresholdComparison,
    threshold: StaffRequestThresholdValue,
    sample: StaffRequestControllerSample,
) -> bool {
    match (threshold, sample) {
        (
            StaffRequestThresholdValue::Number(value),
            StaffRequestControllerSample::NumberQ16(q16),
        ) if value.is_finite() => {
            let threshold_q16 = (f64::from(value) * 65_536.0).round() as i64;
            let current = i64::from(q16);
            match comparison {
                StaffRequestThresholdComparison::EntersEquality => current == threshold_q16,
                StaffRequestThresholdComparison::FallsBelow => current < threshold_q16,
                StaffRequestThresholdComparison::RisesAbove => current > threshold_q16,
                StaffRequestThresholdComparison::ReachesOrBelow => current <= threshold_q16,
                StaffRequestThresholdComparison::ReachesOrAbove => current >= threshold_q16,
            }
        }
        (
            StaffRequestThresholdValue::Boolean(threshold),
            StaffRequestControllerSample::Boolean(value),
        ) if comparison == StaffRequestThresholdComparison::EntersEquality => value == threshold,
        _ => false,
    }
}
