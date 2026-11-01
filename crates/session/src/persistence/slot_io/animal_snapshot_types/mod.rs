use openzt2_game_data::{
    species::{LifeStage, Sex},
    AssetId,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AnimalSnapshotRecord {
    pub persistent_identifier: u64,
    pub species: AssetId,
    pub variant: AssetId,
    pub sex: Sex,
    pub life_stage: LifeStage,
    pub born_tick: u64,
    pub name: Option<String>,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub parents: [Option<u64>; 2],
    /// The father and an unused due tick, kept for older saves.
    pub pregnancy: Option<(u64, u64)>,
    /// A pregnancy whose father is not known.
    #[serde(default)]
    pub pregnant_without_father: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mate: Option<u64>,
    /// Tokens waiting for a task, such as `t_Pregnant` during gestation.
    /// Claimed tokens are saved with the task that claimed them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_tokens: Vec<AnimalPendingTokenSnapshotRecord>,
    pub crated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crate_identifier: Option<u64>,
    pub release_restricted: bool,
    pub crating_restricted: bool,
    pub death: Option<AnimalSnapshotDeath>,
    pub welfare: Option<AnimalWelfareSnapshotRecord>,
}

/// The tuple representation preserves disease records from older snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum AnimalSnapshotDeath {
    Disease(AssetId, u64),
    Authored { program: AssetId, since_tick: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AnimalPendingTokenSnapshotRecord {
    pub name: AssetId,
    pub available_at: u64,
    pub expires_at: u64,
    pub payload: Option<u64>,
    pub retain_on_failure: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AnimalWelfareSnapshotRecord {
    pub needs: [i32; 11],
    /// Absent in older saves; such animals start the need afresh.
    #[serde(default)]
    pub reproduction_need: Option<i32>,
    #[serde(default)]
    pub need_adjustment_timer: Option<(u64, [u64; 2])>,
    pub triggers: u16,
    pub cumulative_points: [f32; 2],
    pub welfare: u16,
    pub habitat_suitability: [u16; 3],
    /// Older records cannot distinguish an unapplied zero score from an assessed
    /// zero score. Reassess those records once instead of inventing a baseline.
    #[serde(default)]
    pub environment_habitat_assessed: bool,
    pub band: u8,
}
