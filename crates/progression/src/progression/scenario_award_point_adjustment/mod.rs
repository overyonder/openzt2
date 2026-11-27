use bevy::prelude::*;

use super::award_and_progression_fact_types::AdjustScenarioAwardPointTotalRequest;
use super::AuthoredAwardPoints;

pub(super) fn apply_requested_scenario_award_point_total_adjustments(
    mut totals: Query<&mut AuthoredAwardPoints>,
    mut requested_adjustments: MessageReader<AdjustScenarioAwardPointTotalRequest>,
) {
    for requested_adjustment in requested_adjustments.read() {
        let Ok(mut points) = totals.get_mut(requested_adjustment.root) else {
            continue;
        };
        let next = points.0.saturating_add_signed(requested_adjustment.delta);
        points.set_if_neq(AuthoredAwardPoints(next));
    }
}
