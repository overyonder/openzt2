use bevy::{ecs::system::SystemParam, prelude::*};

use crate::plugins::simulation_time::{
    deterministic_random_stream::ZooSeed,
    simulation_clock_types::{ZooCalendar, ZooClock},
    simulation_control_types::SimulationControl,
};

use super::persistence_failure_types::WorldSnapshotPersistenceFailure;

pub(super) const SIMULATION_TIME_SNAPSHOT_RECORD_BYTE_COUNT: u64 = 30;

#[derive(SystemParam)]
pub(super) struct SimulationTimeSnapshotSaveResources<'w> {
    clock: Res<'w, ZooClock>,
    calendar: Res<'w, ZooCalendar>,
    control: Res<'w, SimulationControl>,
    seed: Res<'w, ZooSeed>,
}

#[derive(SystemParam)]
pub(super) struct SimulationTimeSnapshotLoadResources<'w> {
    clock: ResMut<'w, ZooClock>,
    calendar: ResMut<'w, ZooCalendar>,
    control: ResMut<'w, SimulationControl>,
    seed: ResMut<'w, ZooSeed>,
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
    pub(super) fn append_encoded_simulation_time_snapshot_record(self, bytes: &mut Vec<u8>) {
        bytes.extend_from_slice(&self.clock.tick.to_le_bytes());
        bytes.extend_from_slice(&self.clock.absolute_day.to_le_bytes());
        bytes.extend_from_slice(&self.clock.tick_in_day.to_le_bytes());
        bytes.extend_from_slice(&self.calendar.year.to_le_bytes());
        bytes.push(self.calendar.month);
        bytes.push(self.calendar.day);
        bytes.push(self.control.speed_tier);
        bytes.push(u8::from(self.control.paused));
        bytes.extend_from_slice(&self.seed.0.to_le_bytes());
    }

    pub(super) fn apply_simulation_time_snapshot_record_to_live_resources(
        self,
        resources: &mut SimulationTimeSnapshotLoadResources,
    ) {
        *resources.clock = self.clock;
        *resources.calendar = self.calendar;
        *resources.control = self.control;
        *resources.seed = self.seed;
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
    if bytes.len() != SIMULATION_TIME_SNAPSHOT_RECORD_BYTE_COUNT as usize {
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
    SimulationTimeSnapshotRecord {
        clock: ZooClock {
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
        control: SimulationControl {
            speed_tier: bytes[20],
            paused,
        },
        seed: ZooSeed(u64_at(22)?),
    }
    .validate_simulation_time_snapshot_record()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_time_state_round_trips() {
        let saved = SimulationTimeSnapshotRecord {
            clock: ZooClock {
                tick: 50,
                absolute_day: 4,
                tick_in_day: 7,
            },
            calendar: ZooCalendar {
                year: 1,
                month: 2,
                day: 5,
            },
            control: SimulationControl {
                speed_tier: 2,
                paused: true,
            },
            seed: ZooSeed(99),
        };
        let mut bytes = Vec::new();
        saved.append_encoded_simulation_time_snapshot_record(&mut bytes);
        assert_eq!(decode_simulation_time_snapshot_record(&bytes), Ok(saved));
    }
}
