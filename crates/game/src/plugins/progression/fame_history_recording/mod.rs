use bevy::prelude::*;

use crate::plugins::simulation_time::simulation_clock_types::ZooCalendar;

use super::{fame_history_types::FameHistory, fame_types::Fame};

pub(super) fn record_current_fame_in_monthly_history(
    calendar: Option<Res<ZooCalendar>>,
    fame: Res<Fame>,
    mut fame_history: ResMut<FameHistory>,
) {
    let Some(calendar) = calendar else {
        return;
    };
    fame_history.record_monthly_value(
        calculate_zero_based_zoo_month_index(*calendar),
        fame.half_stars,
    );
}

fn calculate_zero_based_zoo_month_index(calendar: ZooCalendar) -> u32 {
    u32::from(calendar.month.saturating_sub(1))
        .saturating_add(u32::from(calendar.year.saturating_sub(2001)).saturating_mul(12))
}
