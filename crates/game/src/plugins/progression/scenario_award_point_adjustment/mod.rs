use bevy::prelude::*;

use super::award_and_progression_fact_types::{
    AdjustScenarioAwardPointTotalRequest, ScenarioAwardPointTotal,
};

pub(super) fn apply_requested_scenario_award_point_total_adjustments(
    mut scenario_award_point_total: ResMut<ScenarioAwardPointTotal>,
    mut requested_adjustments: MessageReader<AdjustScenarioAwardPointTotalRequest>,
) {
    for requested_adjustment in requested_adjustments.read() {
        scenario_award_point_total.0 = if requested_adjustment.delta >= 0 {
            scenario_award_point_total
                .0
                .saturating_add(requested_adjustment.delta as u32)
        } else {
            scenario_award_point_total
                .0
                .saturating_sub(requested_adjustment.delta.unsigned_abs())
        };
    }
}
