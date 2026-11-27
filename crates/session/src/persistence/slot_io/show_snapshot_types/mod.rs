use openzt2_game_data::AssetId;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;

#[derive(Debug, Clone)]
pub(super) struct ShowStageSnapshotRecord {
    pub persistent_identifier: PersistentId,
    pub is_open: bool,
    pub display_name: String,
    pub canopy_upgrade_persistent_identifier: Option<PersistentId>,
    pub television_upgrade_persistent_identifier: Option<PersistentId>,
    pub running: Option<RunningShowSnapshot>,
}

#[derive(Debug, Clone)]
pub(super) struct RunningShowSnapshot {
    pub row_id: u64,
    pub phase: u8,
    pub current_slot: u32,
    pub trick_score: f32,
    pub stars: f32,
    pub completed_tricks: u32,
    pub failed_tricks: u32,
    pub remaining_ns: u64,
    pub continuation: Option<RunningShowContinuation>,
}

impl RunningShowSnapshot {
    pub(super) fn phase(&self) -> Option<openzt2_zoo_state::shows::running_show_types::ShowPhase> {
        use openzt2_zoo_state::shows::running_show_types::ShowPhase;
        Some(match self.phase {
            0 => ShowPhase::Preshow,
            1 => ShowPhase::Conducting,
            2 => ShowPhase::Postshow,
            3 => ShowPhase::BetweenShows,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct RunningShowContinuation {
    pub columns: Vec<Vec<u32>>,
    pub wait_ns: u64,
    pub scores: Option<(Vec<(u32, f32)>, Vec<(u32, f32)>)>,
    pub tricks: Vec<ActiveShowTrickSnapshot>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct ActiveShowTrickSnapshot {
    pub slot: u32,
    pub performer: u64,
    pub definition: AssetId,
    pub scored: bool,
    pub pending_token_duration: Option<u32>,
    pub tokens: Vec<ShowTrickTokenSnapshot>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub(super) struct ShowTrickTokenSnapshot {
    pub name: AssetId,
    pub available_at: u64,
    pub expires_at: u64,
    pub execution: Option<u64>,
    pub destination: Option<[f32; 3]>,
    pub payload: Option<u64>,
}

#[derive(Debug, Clone)]
pub(super) struct ScheduledShowSnapshotRecord {
    pub persistent_identifier: PersistentId,
    pub stage_persistent_identifier: PersistentId,
    pub schedule_row_order: u16,
    pub is_active: bool,
    pub performance_capacity: u16,
    pub rating: Option<f32>,
    pub performance_records: Vec<ScheduledShowPerformanceSnapshotRecord>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ScheduledShowPerformanceSnapshotRecord {
    pub performer_persistent_identifier: PersistentId,
    pub trick_definition_identifier: Option<AssetId>,
    pub time_slot: Option<u16>,
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
