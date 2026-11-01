pub(super) mod original_saved_game_conversion;
use bevy::{ecs::system::SystemParam, prelude::*};

use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooCalendar;
use openzt2_world_state::simulation_time::simulation_clock_types::{ZooClock, ZooClockFraction};
use openzt2_world_state::simulation_time::simulation_control_types::SimulationControl;

use super::persistence_failure_types::WorldSnapshotPersistenceFailure;

pub(super) const SIMULATION_TIME_SNAPSHOT_RECORD_BYTE_COUNT: u64 = 46;
const LEGACY_SIMULATION_TIME_SNAPSHOT_RECORD_BYTE_COUNT: usize = 30;

#[derive(SystemParam)]
pub(super) struct SimulationTimeSnapshotSaveResources<'w> {
    clock: Res<'w, ZooClock>,
    calendar: Res<'w, ZooCalendar>,
    control: Res<'w, SimulationControl>,
    seed: Res<'w, ZooSeed>,
}

#[derive(SystemParam)]
pub(super) struct SimulationTimeSnapshotLoadResources<'w> {
    clock: Option<ResMut<'w, ZooClock>>,
    calendar: Option<ResMut<'w, ZooCalendar>>,
    control: Option<ResMut<'w, SimulationControl>>,
    seed: Option<ResMut<'w, ZooSeed>>,
}

impl SimulationTimeSnapshotLoadResources<'_> {
    pub(super) fn is_initialized(&self) -> bool {
        self.clock.is_some()
            && self.calendar.is_some()
            && self.control.is_some()
            && self.seed.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SimulationTimeSnapshotRecord {
    clock: ZooClock,
    calendar: ZooCalendar,
    control: SimulationControl,
    seed: ZooSeed,
}

impl SimulationTimeSnapshotSaveResources<'_> {
    pub(super) fn capture_validated_simulation_time_snapshot_record(
        &self,
    ) -> Result<SimulationTimeSnapshotRecord, WorldSnapshotPersistenceFailure> {
        SimulationTimeSnapshotRecord {
            clock: *self.clock,
            calendar: *self.calendar,
            control: *self.control,
            seed: *self.seed,
        }
        .validate_simulation_time_snapshot_record()
    }
}

impl SimulationTimeSnapshotRecord {
    pub(super) fn validate_against_timing(
        self,
        timing: &openzt2_game_data::world_definitions::simulation_time::SimulationTimingDefinition,
    ) -> Result<Self, WorldSnapshotPersistenceFailure> {
        if timing.ticks_per_day == 0
            || self.clock.tick / u64::from(timing.ticks_per_day) != u64::from(self.clock.absolute_day)
            || self.clock.tick % u64::from(timing.ticks_per_day) != u64::from(self.clock.tick_in_day)
            || openzt2_world_state::simulation_time::zoo_calendar_calculation::calculate_zoo_calendar_for_absolute_day(&timing.calendar, self.clock.absolute_day) != self.calendar
        {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        Ok(self)
    }

    pub(super) fn append_encoded_simulation_time_snapshot_record(self, bytes: &mut Vec<u8>) {
        bytes.extend_from_slice(&self.clock.tick.to_le_bytes());
        bytes.extend_from_slice(&self.clock.absolute_day.to_le_bytes());
        bytes.extend_from_slice(&self.clock.tick_in_day.to_le_bytes());
        bytes.extend_from_slice(&self.calendar.year.to_le_bytes());
        bytes.push(self.calendar.month);
        bytes.push(self.calendar.day);
        bytes.push(self.control.speed_tier);
        // A save made from a dialog must not keep that dialog's temporary pause.
        bytes.push(u8::from(self.control.requested_pause_state()));
        bytes.extend_from_slice(&self.seed.0.to_le_bytes());
        bytes.extend_from_slice(&self.clock.fractional_tick.numerator().to_le_bytes());
        bytes.extend_from_slice(&self.clock.fractional_tick.denominator().to_le_bytes());
    }

    pub(super) fn apply_simulation_time_snapshot_record_to_live_resources(
        self,
        resources: &mut SimulationTimeSnapshotLoadResources,
    ) -> Result<(), WorldSnapshotPersistenceFailure> {
        let (Some(clock), Some(calendar), Some(control), Some(seed)) = (
            resources.clock.as_mut(),
            resources.calendar.as_mut(),
            resources.control.as_mut(),
            resources.seed.as_mut(),
        ) else {
            return Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference);
        };
        **clock = self.clock;
        **calendar = self.calendar;
        **control = self.control;
        **seed = self.seed;
        Ok(())
    }

    fn validate_simulation_time_snapshot_record(
        self,
    ) -> Result<Self, WorldSnapshotPersistenceFailure> {
        (self.calendar.year != 0
            && (1..=12).contains(&self.calendar.month)
            && (1..=31).contains(&self.calendar.day)
            && self.control.speed_tier <= 3)
            .then_some(self)
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
    }
}

pub(super) fn decode_simulation_time_snapshot_record(
    bytes: &[u8],
) -> Result<SimulationTimeSnapshotRecord, WorldSnapshotPersistenceFailure> {
    if bytes.len() != SIMULATION_TIME_SNAPSHOT_RECORD_BYTE_COUNT as usize
        && bytes.len() != LEGACY_SIMULATION_TIME_SNAPSHOT_RECORD_BYTE_COUNT
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let u32_at = |offset| {
        bytes
            .get(offset..offset + 4)
            .and_then(|value| value.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
    };
    let u64_at = |offset| {
        bytes
            .get(offset..offset + 8)
            .and_then(|value| value.try_into().ok())
            .map(u64::from_le_bytes)
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
    };
    let paused = match bytes[21] {
        0 => false,
        1 => true,
        _ => return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
    };
    let fractional_tick = if bytes.len() == LEGACY_SIMULATION_TIME_SNAPSHOT_RECORD_BYTE_COUNT {
        ZooClockFraction::default()
    } else {
        let numerator = u64_at(30)?;
        let denominator = u64_at(38)?;
        let fraction = ZooClockFraction::from_ratio(numerator, denominator)
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        if fraction.numerator() != numerator || fraction.denominator() != denominator {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        fraction
    };
    SimulationTimeSnapshotRecord {
        clock: ZooClock {
            fractional_tick,
            tick: u64_at(0)?,
            absolute_day: u32_at(8)?,
            tick_in_day: u32_at(12)?,
        },
        calendar: ZooCalendar {
            year: u16::from_le_bytes(
                bytes[16..18]
                    .try_into()
                    .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
            ),
            month: bytes[18],
            day: bytes[19],
        },
        control: SimulationControl::new(bytes[20], paused),
        seed: ZooSeed(u64_at(22)?),
    }
    .validate_simulation_time_snapshot_record()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_time_clock() -> ZooClock {
        ZooClock {
            tick: 0,
            absolute_day: 0,
            tick_in_day: 0,
            fractional_tick: Default::default(),
        }
    }

    #[test]
    fn exact_time_state_round_trips() {
        let saved = SimulationTimeSnapshotRecord {
            clock: ZooClock {
                fractional_tick: ZooClockFraction::from_ratio(1, 10).unwrap(),
                tick: 318,
                absolute_day: 0,
                tick_in_day: 318,
            },
            calendar: ZooCalendar {
                year: 1,
                month: 1,
                day: 1,
            },
            control: SimulationControl::new(2, true),
            seed: ZooSeed(99),
        };
        let mut bytes = Vec::new();
        saved.append_encoded_simulation_time_snapshot_record(&mut bytes);
        assert_eq!(decode_simulation_time_snapshot_record(&bytes), Ok(saved));
        let timing = openzt2_game_data::world_definitions::simulation_time::SimulationTimingDefinition {
            fixed_hz: 100, ticks_per_day: 2400, speed_multipliers: Vec::new(),
            calendar: openzt2_game_data::world_definitions::simulation_time::SimulationCalendarPolicy {
                epoch_year: 1, epoch_month: 1, epoch_day: 1,
                month_lengths: [31,28,31,30,31,30,31,31,30,31,30,31],
                leap: openzt2_game_data::world_definitions::simulation_time::SimulationCalendarLeapPolicy::Gregorian,
            },
        };
        assert_eq!(saved.validate_against_timing(&timing), Ok(saved));
        let mut inconsistent = saved;
        inconsistent.clock.absolute_day = 1;
        assert!(inconsistent.validate_against_timing(&timing).is_err());
        inconsistent = saved;
        inconsistent.calendar.day = 2;
        assert!(inconsistent.validate_against_timing(&timing).is_err());
        let mut world = World::new();
        world.insert_resource(legacy_time_clock());
        world.insert_resource(saved.calendar);
        world.insert_resource(SimulationControl::new(0, false));
        world.insert_resource(ZooSeed(0));
        let mut load =
            bevy::ecs::system::SystemState::<SimulationTimeSnapshotLoadResources>::new(&mut world);
        decode_simulation_time_snapshot_record(&bytes)
            .unwrap()
            .apply_simulation_time_snapshot_record_to_live_resources(
                &mut load.get_mut(&mut world).unwrap(),
            )
            .unwrap();
        let mut capture =
            bevy::ecs::system::SystemState::<SimulationTimeSnapshotSaveResources>::new(&mut world);
        assert_eq!(
            capture
                .get(&world)
                .unwrap()
                .capture_validated_simulation_time_snapshot_record(),
            Ok(saved)
        );
        let legacy = decode_simulation_time_snapshot_record(&bytes[..30]).unwrap();
        assert_eq!(legacy.clock.fractional_tick, ZooClockFraction::default());
        assert_eq!(legacy.clock.tick, saved.clock.tick);
        let mut malformed = bytes.clone();
        malformed[38..46].copy_from_slice(&0_u64.to_le_bytes());
        assert!(decode_simulation_time_snapshot_record(&malformed).is_err());
        malformed[30..38].copy_from_slice(&2_u64.to_le_bytes());
        malformed[38..46].copy_from_slice(&20_u64.to_le_bytes());
        assert!(decode_simulation_time_snapshot_record(&malformed).is_err());
        assert!(decode_simulation_time_snapshot_record(&bytes[..45]).is_err());
    }
}
