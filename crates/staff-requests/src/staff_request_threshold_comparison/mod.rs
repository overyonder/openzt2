use super::staff_request_state_types::StaffRequestControllerSample;
use openzt2_game_data::world_definitions::staff_management::StaffRequestThresholdComparison;
use openzt2_game_data::world_definitions::staff_management::StaffRequestThresholdValue;

pub(super) fn evaluate_staff_threshold(
    comparison: StaffRequestThresholdComparison,
    threshold: StaffRequestThresholdValue,
    previous: Option<StaffRequestControllerSample>,
    current: StaffRequestControllerSample,
) -> i8 {
    let current_active = predicate(comparison, threshold, current).unwrap_or(false);
    if !current_active {
        0
    } else if previous
        .and_then(|sample| predicate(comparison, threshold, sample))
        .unwrap_or(false)
    {
        -1
    } else {
        1
    }
}

fn predicate(
    comparison: StaffRequestThresholdComparison,
    threshold: StaffRequestThresholdValue,
    sample: StaffRequestControllerSample,
) -> Option<bool> {
    match (threshold, sample) {
        (
            StaffRequestThresholdValue::Number(value),
            StaffRequestControllerSample::NumberQ16(q16),
        ) if value.is_finite() => {
            let threshold_q16 = (f64::from(value) * 65_536.0).round() as i64;
            let current = i64::from(q16);
            Some(match comparison {
                StaffRequestThresholdComparison::EntersEquality => current == threshold_q16,
                StaffRequestThresholdComparison::FallsBelow => current < threshold_q16,
                StaffRequestThresholdComparison::RisesAbove => current > threshold_q16,
                StaffRequestThresholdComparison::ReachesOrBelow => current <= threshold_q16,
                StaffRequestThresholdComparison::ReachesOrAbove => current >= threshold_q16,
            })
        }
        (
            StaffRequestThresholdValue::Boolean(threshold),
            StaffRequestControllerSample::Boolean(value),
        ) if comparison == StaffRequestThresholdComparison::EntersEquality => {
            Some(value == threshold)
        }
        _ => None,
    }
}
