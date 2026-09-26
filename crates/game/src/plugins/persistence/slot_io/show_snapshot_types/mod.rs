use openzt2_game_data::AssetId;

use crate::plugins::world_spawn::persistent_id_types::PersistentId;

#[derive(Debug, Clone)]
pub(super) struct ShowStageSnapshotRecord {
    pub persistent_identifier: PersistentId,
    pub is_open: bool,
    pub display_name: String,
    pub canopy_upgrade_persistent_identifier: Option<PersistentId>,
    pub television_upgrade_persistent_identifier: Option<PersistentId>,
}

#[derive(Debug, Clone)]
pub(super) struct ScheduledShowSnapshotRecord {
    pub persistent_identifier: PersistentId,
    pub stage_persistent_identifier: PersistentId,
    pub schedule_row_order: u16,
    pub is_active: bool,
    pub performance_capacity: u16,
    pub performance_records: Vec<ScheduledShowPerformanceSnapshotRecord>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ScheduledShowPerformanceSnapshotRecord {
    pub performer_persistent_identifier: PersistentId,
    pub trick_definition_identifier: AssetId,
    pub duration_simulation_ticks: u32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ScheduledShowBreakSnapshotRecord {
    pub persistent_identifier: PersistentId,
    pub stage_persistent_identifier: PersistentId,
    pub schedule_row_order: u16,
    pub is_active: bool,
    pub duration_simulation_ticks: u32,
}

#[derive(Debug)]
pub(super) struct ShowSnapshotRecords {
    pub stage_records: Vec<ShowStageSnapshotRecord>,
    pub scheduled_show_records: Vec<ScheduledShowSnapshotRecord>,
    pub scheduled_break_records: Vec<ScheduledShowBreakSnapshotRecord>,
}
