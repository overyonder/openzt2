use super::{
    tank_snapshot_types::TankSnapshotRecord,
    world_snapshot_capture_system_parameters::WorldSnapshotCaptureQueries,
};

pub(super) fn capture_sorted_tank_snapshot_records_from_live_world(
    world_snapshot_capture_queries: &WorldSnapshotCaptureQueries,
) -> Vec<TankSnapshotRecord> {
    let mut tank_snapshot_records = world_snapshot_capture_queries
        .tanks_with_snapshot_state
        .iter()
        .map(
            |(persistent_identifier, tank, tank_geometry, optional_water_quality)| {
                TankSnapshotRecord {
                    persistent_identifier: *persistent_identifier,
                    tank_definition_identifier: tank.definition,
                    geometry: *tank_geometry,
                    water_quality: optional_water_quality.copied().unwrap_or_default(),
                }
            },
        )
        .collect::<Vec<_>>();
    tank_snapshot_records.sort_unstable_by_key(|record| record.persistent_identifier.0);
    tank_snapshot_records
}
