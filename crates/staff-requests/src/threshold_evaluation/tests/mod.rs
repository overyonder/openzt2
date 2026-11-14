use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::*;
use openzt2_game_data::world_definitions::staff_management::{
    StaffRequestControllerDefinition, StaffRequestData,
    StaffRequestThresholdComparison as Comparison, StaffRequestThresholdValue,
};

fn number_row(
    comparison: Comparison,
    trigger_on_creation: bool,
) -> StaffRequestControllerDefinition {
    StaffRequestControllerDefinition {
        id: AssetId::from_key("fooddish"),
        binder: Some(AssetId::from_key("fill")),
        attribute_key: Some(AssetId::from_key("f_foodlevel")),
        threshold: StaffRequestThresholdValue::Number(25.0),
        threshold_comparison: comparison,
        cancel_threshold: StaffRequestThresholdValue::Number(50.0),
        cancel_comparison: Comparison::RisesAbove,
        trigger_on_creation,
        trigger_on_destruction: false,
        request: StaffRequestData {
            token: Some(AssetId::from_key("t_fillfoodcontainer")),
            subject_type: None,
            priority: 4.0,
            target: None,
            staff: None,
        },
    }
}

#[test]
fn finite_threshold_results_are_exact_signed_edges() {
    let cases = [
        (Comparison::EntersEquality, 25, 24, 1),
        (Comparison::EntersEquality, 25, 25, -1),
        (Comparison::FallsBelow, 24, 25, 1),
        (Comparison::FallsBelow, 24, 24, -1),
        (Comparison::RisesAbove, 26, 25, 1),
        (Comparison::RisesAbove, 26, 26, -1),
        (Comparison::ReachesOrBelow, 25, 26, 1),
        (Comparison::ReachesOrBelow, 25, 25, -1),
        (Comparison::ReachesOrAbove, 25, 24, 1),
        (Comparison::ReachesOrAbove, 25, 25, -1),
    ];
    for (comparison, current, previous, expected) in cases {
        assert_eq!(
            evaluate_staff_threshold(
                comparison,
                StaffRequestThresholdValue::Number(25.0),
                Some(StaffRequestControllerSample::NumberQ16(previous * 65_536)),
                StaffRequestControllerSample::NumberQ16(current * 65_536),
            ),
            expected
        );
    }
    assert_eq!(
        evaluate_staff_threshold(
            Comparison::FallsBelow,
            StaffRequestThresholdValue::Number(25.0),
            Some(StaffRequestControllerSample::NumberQ16(24 * 65_536)),
            StaffRequestControllerSample::NumberQ16(26 * 65_536),
        ),
        0
    );
}

#[test]
fn authored_hysteresis_uses_independent_cancel_threshold() {
    let row = number_row(Comparison::FallsBelow, false);
    let target = Entity::from_bits(8);
    let mut state = StaffRequestRuntimeState {
        row: StaffRequestRowIdentity {
            definition: row.id,
            binder: row.binder,
            attribute_key: row.attribute_key,
            token: row.request.token,
            ordinal: 0,
        },
        previous: None,
    };
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestControllerSample::NumberQ16(26 * 65_536))
    )
    .is_none());
    assert!(matches!(
        evaluate_staff_request_row(
            &row,
            &mut state,
            target,
            Some(StaffRequestControllerSample::NumberQ16(24 * 65_536))
        ),
        Some(StaffRequestJobEvent::Raised { .. })
    ));
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestControllerSample::NumberQ16(24 * 65_536))
    )
    .is_none());
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestControllerSample::NumberQ16(25 * 65_536))
    )
    .is_none());
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestControllerSample::NumberQ16(50 * 65_536))
    )
    .is_none());
    assert!(matches!(
        evaluate_staff_request_row(
            &row,
            &mut state,
            target,
            Some(StaffRequestControllerSample::NumberQ16(51 * 65_536))
        ),
        Some(StaffRequestJobEvent::Cancelled { .. })
    ));
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestControllerSample::NumberQ16(51 * 65_536))
    )
    .is_none());
}

#[test]
fn creation_and_edges_emit_one_request_and_one_cancellation() {
    let row = number_row(Comparison::FallsBelow, true);
    let target = Entity::from_bits(7);
    let mut state = StaffRequestRuntimeState {
        row: StaffRequestRowIdentity {
            definition: row.id,
            binder: row.binder,
            attribute_key: row.attribute_key,
            token: row.request.token,
            ordinal: 0,
        },
        previous: None,
    };
    assert!(matches!(
        evaluate_staff_request_row(
            &row,
            &mut state,
            target,
            Some(StaffRequestControllerSample::NumberQ16(24 * 65_536)),
        ),
        Some(StaffRequestJobEvent::Raised { .. })
    ));
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestControllerSample::NumberQ16(23 * 65_536)),
    )
    .is_none());
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestControllerSample::NumberQ16(25 * 65_536)),
    )
    .is_none());
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestControllerSample::NumberQ16(50 * 65_536)),
    )
    .is_none());
    assert!(matches!(
        evaluate_staff_request_row(
            &row,
            &mut state,
            target,
            Some(StaffRequestControllerSample::NumberQ16(51 * 65_536)),
        ),
        Some(StaffRequestJobEvent::Cancelled { .. })
    ));
}

#[test]
fn a_filter_is_cleaned_at_the_thresholds_of_its_chosen_schedule() {
    use openzt2_game_data::world_definitions::world_objects::WaterFilterPolicy;
    use openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterFilter;
    use openzt2_zoo_state::economy::facility_economy_types::MaintenanceScheduleIndex;
    let mut row = number_row(Comparison::RisesAbove, false);
    row.attribute_key = Some(AssetId::from_key("f_filterdirtylevel"));
    let filter = WaterFilter {
        policy: WaterFilterPolicy {
            cleanup_interval_seconds: 2.0,
            cleanup_rate: 750.0,
            maximum_dirt_capacity: 75_000.0,
            maintenance_thresholds: Some([[20.0, 10.0], [50.0, 10.0], [80.0, 10.0]]),
        },
        seconds_until_cleanup: 0.0,
        dirt_percent: 0.0,
    };
    let thresholds = |schedule| {
        filter_schedule_row(&row, &filter, schedule)
            .map(|row| (row.threshold, row.cancel_threshold))
    };
    // The panel starts on regular.
    assert_eq!(
        thresholds(None),
        Some((
            StaffRequestThresholdValue::Number(50.0),
            StaffRequestThresholdValue::Number(10.0)
        ))
    );
    assert_eq!(
        thresholds(Some(&MaintenanceScheduleIndex(0))),
        Some((
            StaffRequestThresholdValue::Number(20.0),
            StaffRequestThresholdValue::Number(10.0)
        ))
    );
    assert_eq!(
        thresholds(Some(&MaintenanceScheduleIndex(2))).map(|(threshold, _)| threshold),
        Some(StaffRequestThresholdValue::Number(80.0))
    );
    // Other requests keep their authored thresholds.
    assert!(
        filter_schedule_row(&number_row(Comparison::RisesAbove, false), &filter, None).is_none()
    );
}
