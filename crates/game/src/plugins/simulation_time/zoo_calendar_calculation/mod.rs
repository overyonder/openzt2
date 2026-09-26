use openzt2_game_data::world_definitions::simulation_time::{
    SimulationCalendarLeapPolicy, SimulationCalendarPolicy,
};

use super::simulation_clock_types::ZooCalendar;

pub(crate) fn calculate_zoo_calendar_for_absolute_day(
    policy: &SimulationCalendarPolicy,
    absolute_day: u32,
) -> ZooCalendar {
    let epoch_ordinal = days_before_year(policy, u32::from(policy.epoch_year))
        + days_before_month(policy, u32::from(policy.epoch_year), policy.epoch_month)
        + u64::from(policy.epoch_day - 1);
    let target_ordinal = epoch_ordinal.saturating_add(u64::from(absolute_day));

    let mut lower_year = 1_u32;
    let mut upper_year = u32::from(u16::MAX);
    while lower_year < upper_year {
        let middle_year = lower_year + (upper_year - lower_year).div_ceil(2);
        if days_before_year(policy, middle_year) <= target_ordinal {
            lower_year = middle_year;
        } else {
            upper_year = middle_year - 1;
        }
    }
    let year = lower_year;
    let year_days = u64::from(days_in_year(policy, year));
    let mut day_in_year = target_ordinal
        .saturating_sub(days_before_year(policy, year))
        .min(year_days - 1);
    let mut month = 1_u8;
    while month < 12 {
        let month_days = u64::from(days_in_month(policy, year, month));
        if day_in_year < month_days {
            break;
        }
        day_in_year -= month_days;
        month += 1;
    }

    ZooCalendar {
        year: year as u16,
        month,
        day: day_in_year as u8 + 1,
    }
}

fn days_before_year(policy: &SimulationCalendarPolicy, year: u32) -> u64 {
    let preceding_years = year.saturating_sub(1);
    let ordinary_year_days = u64::from(
        policy
            .month_lengths
            .iter()
            .map(|days| u16::from(*days))
            .sum::<u16>(),
    );
    let leap_days = if matches!(policy.leap, SimulationCalendarLeapPolicy::Gregorian) {
        preceding_years / 4 - preceding_years / 100 + preceding_years / 400
    } else {
        0
    };
    u64::from(preceding_years) * ordinary_year_days + u64::from(leap_days)
}

fn days_before_month(policy: &SimulationCalendarPolicy, year: u32, month: u8) -> u64 {
    (1..month)
        .map(|month| u64::from(days_in_month(policy, year, month)))
        .sum()
}

fn days_in_year(policy: &SimulationCalendarPolicy, year: u32) -> u16 {
    policy
        .month_lengths
        .iter()
        .map(|days| u16::from(*days))
        .sum::<u16>()
        + u16::from(
            matches!(policy.leap, SimulationCalendarLeapPolicy::Gregorian)
                && is_gregorian_leap_year(year),
        )
}

fn days_in_month(policy: &SimulationCalendarPolicy, year: u32, month: u8) -> u8 {
    let ordinary_month_days = policy.month_lengths[usize::from(month - 1)];
    if matches!(policy.leap, SimulationCalendarLeapPolicy::Gregorian)
        && month == 2
        && is_gregorian_leap_year(year)
    {
        ordinary_month_days.saturating_add(1)
    } else {
        ordinary_month_days
    }
}

const fn is_gregorian_leap_year(year: u32) -> bool {
    year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
}
