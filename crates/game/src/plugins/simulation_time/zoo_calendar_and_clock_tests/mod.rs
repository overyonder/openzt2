use openzt2_game_data::world_definitions::simulation_time::{
    SimulationCalendarLeapPolicy, SimulationCalendarPolicy,
};

use super::{
    simulation_clock_types::{ZooCalendar, ZooClock},
    zoo_calendar_calculation::calculate_zoo_calendar_for_absolute_day,
    zoo_clock_advancement::advance_zoo_clock_by_one_tick,
};

const TEST_SIMULATION_CALENDAR_POLICY: SimulationCalendarPolicy = SimulationCalendarPolicy {
    epoch_year: 2001,
    epoch_month: 1,
    epoch_day: 1,
    month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
    leap: SimulationCalendarLeapPolicy::Gregorian,
};

#[test]
fn tick_day_deadline_and_calendar_boundaries_are_exact() {
    let mut zoo_clock = ZooClock {
        tick: 58,
        absolute_day: 58,
        tick_in_day: 9,
    };
    assert!(advance_zoo_clock_by_one_tick(&mut zoo_clock, 10));
    assert_eq!(
        zoo_clock,
        ZooClock {
            tick: 59,
            absolute_day: 59,
            tick_in_day: 0,
        }
    );
    assert_eq!(
        calculate_zoo_calendar_for_absolute_day(
            &TEST_SIMULATION_CALENDAR_POLICY,
            zoo_clock.absolute_day,
        ),
        ZooCalendar {
            year: 2001,
            month: 3,
            day: 1,
        }
    );
    let tick_deadline = zoo_clock.tick + 25;
    for _ in 0..25 {
        assert!(advance_zoo_clock_by_one_tick(&mut zoo_clock, 10));
    }
    assert_eq!(zoo_clock.tick, tick_deadline);
}

#[test]
fn gregorian_calendar_policy_handles_century_and_four_hundred_year_rules() {
    let four_hundred_year_policy = SimulationCalendarPolicy {
        epoch_year: 1999,
        ..TEST_SIMULATION_CALENDAR_POLICY
    };
    assert_eq!(
        calculate_zoo_calendar_for_absolute_day(&four_hundred_year_policy, 424),
        ZooCalendar {
            year: 2000,
            month: 2,
            day: 29,
        }
    );
    let non_leap_century_policy = SimulationCalendarPolicy {
        epoch_year: 2099,
        ..TEST_SIMULATION_CALENDAR_POLICY
    };
    assert_eq!(
        calculate_zoo_calendar_for_absolute_day(&non_leap_century_policy, 424),
        ZooCalendar {
            year: 2100,
            month: 3,
            day: 1,
        }
    );
}
