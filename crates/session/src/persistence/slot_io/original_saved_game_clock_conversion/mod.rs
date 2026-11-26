use std::io;

use openzt2_game_assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use openzt2_game_data::world_definitions::simulation_time::{
    SimulationCalendarLeapPolicy, SimulationTimingDefinition,
};
use openzt2_world_state::simulation_time::{
    simulation_clock_types::{ZooCalendar, ZooClock, ZooDayCadence},
    zoo_calendar_calculation::calculate_zoo_calendar_for_absolute_day,
};

const CALENDAR_EPOCH_FILE_TIME: u64 = 126_227_808_000_000_000;
#[cfg(test)]
const FILE_TIME_UNITS_PER_DAY: u64 = 864_000_000_000;

pub(super) fn read_original_saved_clock(
    status: &OrderedSourceDocumentNode,
    timing: &SimulationTimingDefinition,
    cadence: ZooDayCadence,
) -> io::Result<(ZooClock, ZooCalendar)> {
    if timing.calendar.leap != SimulationCalendarLeapPolicy::Gregorian
        || timing.calendar.epoch_year % 400 != 1
        || timing.calendar.epoch_month != 1
        || timing.calendar.epoch_day != 1
        || timing.calendar.month_lengths != [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "saved calendar requires a Gregorian January epoch in the same 400-year cycle as 2001",
        ));
    }
    let read_word = |name| -> io::Result<u32> {
        let source = status.attribute(name).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("ZTStatus has no {name}"),
            )
        })?;
        source
            .parse::<u32>()
            .or_else(|_| {
                source
                    .parse::<i32>()
                    .map(|value| u32::from_le_bytes(value.to_le_bytes()))
            })
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("ZTStatus {name} is not a 32-bit word"),
                )
            })
    };
    let file_time =
        u64::from(read_word("zooTimeHigh")?) << 32 | u64::from(read_word("zooTimeLow")?);
    let elapsed = file_time
        .checked_sub(CALENDAR_EPOCH_FILE_TIME)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "saved calendar precedes its 2001 epoch",
            )
        })?;
    let duration =
        std::time::Duration::new(elapsed / 10_000_000, ((elapsed % 10_000_000) * 100) as u32);
    let clock = cadence.clock_at_elapsed_zoo_time(duration).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "saved calendar exceeds the fixed clock",
        )
    })?;
    Ok((
        clock,
        calculate_zoo_calendar_for_absolute_day(&timing.calendar, clock.absolute_day),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_assets::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };
    use openzt2_game_data::world_definitions::simulation_time::SimulationCalendarPolicy;
    use openzt2_world_state::simulation_time::simulation_clock_types::ZooClockFraction;

    #[test]
    fn file_time_restores_fractional_tick_and_calendar_without_rounding() -> io::Result<()> {
        let timing = SimulationTimingDefinition {
            fixed_hz: 100,
            ticks_per_day: 2400,
            speed_multipliers: Vec::new(),
            calendar: SimulationCalendarPolicy {
                epoch_year: 1,
                epoch_month: 1,
                epoch_day: 1,
                month_lengths: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31],
                leap: SimulationCalendarLeapPolicy::Gregorian,
            },
        };
        let cadence = ZooDayCadence::from_ticks_per_day(
            std::num::NonZeroU32::new(timing.ticks_per_day).unwrap(),
        );
        let status = |elapsed| {
            let time = CALENDAR_EPOCH_FILE_TIME + elapsed;
            parse_blue_fang_source_document(
                AssetPath::new("saved/world.zt2"),
                format!(
                    "<ZTStatus zooTimeHigh=\"{}\" zooTimeLow=\"{}\"/>",
                    time >> 32,
                    time as i32
                )
                .as_bytes(),
            )
            .map_err(io::Error::other)
        };
        let epoch = status(0)?;
        assert!(epoch.root.attribute("zooTimeLow").unwrap().starts_with('-'));
        assert_eq!(
            read_original_saved_clock(&epoch.root, &timing, cadence)?
                .0
                .tick,
            0
        );
        let document = status(114_516_000_000)?;
        let (clock, calendar) = read_original_saved_clock(&document.root, &timing, cadence)?;
        assert_eq!(
            (clock.tick, clock.absolute_day, clock.tick_in_day),
            (318, 0, 318)
        );
        assert_eq!(
            clock.fractional_tick,
            ZooClockFraction::from_ratio(1, 10).unwrap()
        );
        assert_eq!((calendar.year, calendar.month, calendar.day), (1, 1, 1));
        let document = status(1155 * FILE_TIME_UNITS_PER_DAY)?;
        let (_, calendar) = read_original_saved_clock(&document.root, &timing, cadence)?;
        assert_eq!((calendar.year, calendar.month, calendar.day), (4, 3, 1));
        Ok(())
    }
}
