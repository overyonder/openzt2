use super::{SimulationTimeSnapshotRecord, WorldSnapshotPersistenceFailure};
use openzt2_game_data::world_definitions::simulation_time::SimulationTimingDefinition;
use openzt2_world_state::simulation_time::{
    deterministic_random_stream::ZooSeed,
    simulation_clock_types::{ZooCalendar, ZooClock, ZooDayCadence},
    simulation_control_types::SimulationSpeedTier,
};

pub(in crate::persistence) fn from_saved_clock(
    clock: ZooClock,
    calendar: ZooCalendar,
    seed: ZooSeed,
    timing: &SimulationTimingDefinition,
    cadence: ZooDayCadence,
) -> Result<SimulationTimeSnapshotRecord, WorldSnapshotPersistenceFailure> {
    SimulationTimeSnapshotRecord {
        clock,
        calendar,
        speed: SimulationSpeedTier(0),
        requested_pause: false,
        seed,
    }
    .validate_simulation_time_snapshot_record()?
    .validate_against_timing(timing, cadence, Some(0))
}
