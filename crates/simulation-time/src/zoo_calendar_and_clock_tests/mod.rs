use openzt2_game_data::world_definitions::simulation_time::{
    SimulationCalendarLeapPolicy, SimulationCalendarPolicy,
};

use super::{
    simulation_clock_types::{ZooCalendar, ZooClock, ZooDayCadence},
    zoo_calendar_calculation::calculate_zoo_calendar_for_absolute_day,
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
    let cadence = ZooDayCadence::new(std::time::Duration::from_secs(1), 8_640.0).unwrap();
    let zoo_clock = cadence.clock_at(589, Default::default()).unwrap();
    let mut zoo_clock = cadence.next_clock(zoo_clock).unwrap();
    assert_eq!(
        zoo_clock,
        ZooClock {
            fractional_tick: Default::default(),
            tick: 590,
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
        zoo_clock = cadence.next_clock(zoo_clock).unwrap();
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

#[test]
fn fractional_clock_phase_survives_tick_and_day_advancement() {
    use openzt2_world_state::simulation_time::simulation_clock_types::ZooClockFraction;
    let phase = ZooClockFraction::from_ratio(1, 10).unwrap();
    let clock = ZooClock {
        tick: 2399,
        absolute_day: 0,
        tick_in_day: 2399,
        fractional_tick: phase,
    };
    let cadence = ZooDayCadence::new(std::time::Duration::from_millis(10), 3_600.0).unwrap();
    let clock = cadence.next_clock(clock).unwrap();
    assert_eq!(
        (clock.tick, clock.absolute_day, clock.tick_in_day),
        (2400, 1, 0)
    );
    assert_eq!(clock.fractional_tick, phase);
    let clock = cadence.next_clock(clock).unwrap();
    assert_eq!(clock.fractional_tick, phase);
    assert!(ZooClockFraction::from_ratio(1, 0).is_none());
    assert!(ZooClockFraction::from_ratio(1, 1).is_none());
    assert_eq!(ZooClockFraction::from_ratio(2, 20), Some(phase));
}

#[test]
fn fractional_day_length_preserves_the_rollover_remainder() -> Result<(), &'static str> {
    let cadence = ZooDayCadence::new(std::time::Duration::from_millis(17), 3_600.0)
        .ok_or("authored 17 ms cadence")?;
    let day = |tick| {
        cadence
            .clock_at(tick, Default::default())
            .map(|clock| clock.absolute_day)
    };
    assert_eq!(day(1411), Some(0));
    assert_eq!(day(1412), Some(1));
    assert_eq!(day(7058), Some(4));
    assert_eq!(day(7059), Some(5));
    Ok(())
}
