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
        property_binding: None,
        threshold: StaffRequestThresholdValue::Number(25.0),
        threshold_comparison: comparison,
        cancel_threshold: StaffRequestThresholdValue::Number(50.0),
        cancel_comparison: Comparison::RisesAbove,
        trigger_on_creation,
        trigger_on_destruction: false,
        request: StaffRequestData {
            token: Some(AssetId::from_key("t_fillfoodcontainer")),
            subject_type: Some(AssetId::from_key("keeper")),
            priority: 4.0,
            target: None,
            staff: None,
        },
    }
}

#[test]
fn finite_thresholds_raise_only_on_entry() {
    let cases = [
        (Comparison::EntersEquality, 25, 24, true),
        (Comparison::EntersEquality, 25, 25, false),
        (Comparison::FallsBelow, 24, 25, true),
        (Comparison::FallsBelow, 24, 24, false),
        (Comparison::FallsBelow, 26, 24, false),
        (Comparison::RisesAbove, 26, 25, true),
        (Comparison::RisesAbove, 26, 26, false),
        (Comparison::ReachesOrBelow, 25, 26, true),
        (Comparison::ReachesOrBelow, 25, 25, false),
        (Comparison::ReachesOrAbove, 25, 24, true),
        (Comparison::ReachesOrAbove, 25, 25, false),
    ];
    let target = Entity::from_bits(7);
    let sample = |value| Some(StaffRequestFactValue::Number(value as f32));
    for (comparison, current, previous, expected) in cases {
        let row = number_row(comparison, false);
        let mut state = bind_rows(
            row.id,
            std::slice::from_ref(&row),
            terminal_job_kind_for_token,
        )
        .remove(0);
        let _ = evaluate_staff_request_row(&row, &mut state, target, sample(previous));
        assert_eq!(
            matches!(
                evaluate_staff_request_row(&row, &mut state, target, sample(current)),
                Some(StaffRequestJobEvent::Raised { .. })
            ),
            expected
        );
    }
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
            routing: StaffJobRouting {
                subject_type: row.request.subject_type,
                recipient: row.request.staff,
            },
            ordinal: 0,
        },
        row_index: 0,
        job_kind: row.request.token.and_then(terminal_job_kind_for_token),
        previous: None,
        created: false,
    };
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestFactValue::Number(26.0))
    )
    .is_none());
    assert!(matches!(
        evaluate_staff_request_row(
            &row,
            &mut state,
            target,
            Some(StaffRequestFactValue::Number(24.0))
        ),
        Some(StaffRequestJobEvent::Raised { .. })
    ));
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestFactValue::Number(24.0))
    )
    .is_none());
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestFactValue::Number(25.0))
    )
    .is_none());
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestFactValue::Number(50.0))
    )
    .is_none());
    assert!(matches!(
        evaluate_staff_request_row(
            &row,
            &mut state,
            target,
            Some(StaffRequestFactValue::Number(51.0))
        ),
        Some(StaffRequestJobEvent::Cancelled { .. })
    ));
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestFactValue::Number(51.0))
    )
    .is_none());
}

#[test]
fn creation_and_edges_emit_one_request_and_one_cancellation_without_a_terminal_kind() {
    let mut row = number_row(Comparison::FallsBelow, true);
    row.request.token = Some(AssetId::from_key("t_checkupbaby"));
    let target = Entity::from_bits(7);
    let mut state = StaffRequestRuntimeState {
        row: StaffRequestRowIdentity {
            definition: row.id,
            binder: row.binder,
            attribute_key: row.attribute_key,
            token: row.request.token,
            routing: StaffJobRouting {
                subject_type: row.request.subject_type,
                recipient: row.request.staff,
            },
            ordinal: 0,
        },
        row_index: 0,
        job_kind: None,
        previous: None,
        created: false,
    };
    assert!(matches!(
        evaluate_staff_request_row(
            &row,
            &mut state,
            target,
            Some(StaffRequestFactValue::Number(24.0)),
        ),
        Some(StaffRequestJobEvent::Raised { .. })
    ));
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestFactValue::Number(23.0)),
    )
    .is_none());
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestFactValue::Number(25.0)),
    )
    .is_none());
    assert!(evaluate_staff_request_row(
        &row,
        &mut state,
        target,
        Some(StaffRequestFactValue::Number(50.0)),
    )
    .is_none());
    assert!(matches!(
        evaluate_staff_request_row(
            &row,
            &mut state,
            target,
            Some(StaffRequestFactValue::Number(51.0)),
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
    row.request.token = Some(AssetId::from_key("t_cleanwaterfilter"));
    row.cancel_comparison = Comparison::FallsBelow;
    let filter = WaterFilter {
        policy: WaterFilterPolicy {
            cleanup_interval_seconds: 2.0,
            cleanup_rate: 750.0,
            maximum_dirt_capacity: 75_000.0,
            full_filter_message: None,
            maintenance_thresholds: Some([[20.0, 10.0], [50.0, 10.0], [80.0, 10.0]]),
        },
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

    let regular = filter_schedule_row(&row, &filter, None).unwrap();
    let preemptive =
        filter_schedule_row(&row, &filter, Some(&MaintenanceScheduleIndex(0))).unwrap();
    let target = Entity::from_bits(7);
    let rows = std::slice::from_ref(&row);
    let mut state = bind_rows(row.id, rows, terminal_job_kind_for_token).remove(0);
    let dirt = |percent| Some(StaffRequestFactValue::Number(percent as f32));
    assert!(evaluate_staff_request_row(&regular, &mut state, target, dirt(40)).is_none());
    assert!(evaluate_staff_request_row(&preemptive, &mut state, target, dirt(40)).is_none());
    let mut state = reconcile_rows(row.id, rows, &[state], terminal_job_kind_for_token).remove(0);
    assert!(matches!(
        evaluate_staff_request_row(&regular, &mut state, target, dirt(55)),
        Some(StaffRequestJobEvent::Raised { .. })
    ));
}

#[test]
fn disease_updates_emit_only_on_value_changes() {
    let mut row = number_row(Comparison::EntersEquality, false);
    row.attribute_key = Some(AssetId::from_key("b_disease"));
    row.threshold = StaffRequestThresholdValue::Boolean(true);
    row.cancel_threshold = StaffRequestThresholdValue::Boolean(false);
    row.cancel_comparison = Comparison::EntersEquality;
    let owner = Entity::from_bits(7);
    let mut state = bind_rows(row.id, &[row], terminal_job_kind_for_token).remove(0);
    let mut apply = |value| {
        evaluate_staff_request_row(
            &row,
            &mut state,
            owner,
            Some(StaffRequestFactValue::Boolean(value)),
        )
    };
    assert!(apply(false).is_none());
    assert!(matches!(
        apply(true),
        Some(StaffRequestJobEvent::Raised { .. })
    ));
    assert!(apply(true).is_none());
    assert!(matches!(
        apply(false),
        Some(StaffRequestJobEvent::Cancelled { .. })
    ));
}

#[test]
fn authored_mod_key_prefers_request_at_a_shared_crossing_and_creation_needs_no_value() {
    let mut row = number_row(Comparison::ReachesOrAbove, false);
    row.attribute_key = Some(AssetId::from_key("f_mod_property"));
    row.threshold = StaffRequestThresholdValue::Number(-2.0);
    row.cancel_threshold = row.threshold;
    row.cancel_comparison = row.threshold_comparison;
    let owner = Entity::from_bits(7);
    let mut state = bind_rows(row.id, &[row], terminal_job_kind_for_token).remove(0);
    let mut apply = |value| {
        evaluate_staff_request_row(
            &row,
            &mut state,
            owner,
            Some(StaffRequestFactValue::Number(value)),
        )
    };
    assert!(apply(-3.0).is_none());
    assert!(matches!(
        apply(-2.0),
        Some(StaffRequestJobEvent::Raised { .. })
    ));
    assert!(apply(-1.0).is_none());
    row.attribute_key = None;
    row.trigger_on_creation = true;
    let mut state = bind_rows(row.id, &[row], terminal_job_kind_for_token).remove(0);
    assert!(matches!(
        evaluate_staff_request_row(&row, &mut state, owner, None),
        Some(StaffRequestJobEvent::Raised { .. })
    ));
    assert!(evaluate_staff_request_row(&row, &mut state, owner, None).is_none());
}
