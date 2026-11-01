use super::{SimulationTimeSnapshotRecord, WorldSnapshotPersistenceFailure};
use openzt2_game_data::world_definitions::simulation_time::SimulationTimingDefinition;
use openzt2_world_state::simulation_time::{
    deterministic_random_stream::ZooSeed,
    simulation_clock_types::{ZooCalendar, ZooClock},
    simulation_control_types::SimulationControl,
};

pub(in crate::persistence) fn from_saved_clock(
    clock: ZooClock,
    calendar: ZooCalendar,
    control: SimulationControl,
    seed: ZooSeed,
    timing: &SimulationTimingDefinition,
) -> Result<SimulationTimeSnapshotRecord, WorldSnapshotPersistenceFailure> {
    SimulationTimeSnapshotRecord {
        clock,
        calendar,
        control,
        seed,
    }
    .validate_simulation_time_snapshot_record()?
    .validate_against_timing(timing)
}
