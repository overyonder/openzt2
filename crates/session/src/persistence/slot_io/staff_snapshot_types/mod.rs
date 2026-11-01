use openzt2_game_data::AssetId;
use serde::{Deserialize, Serialize};

/// One employee. The look and name are redrawn from the role and
/// persistent id, so only the live employment state and pose are stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StaffSnapshotRecord {
    pub persistent_identifier: u64,
    pub definition: AssetId,
    /// The employee's role. Roles can share an object, so the definition alone
    /// does not identify it.
    pub role: AssetId,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub wage_cents_per_month: i64,
    pub hired_tick: u64,
    pub hired_month_ordinal: u32,
    /// `b_CaffeineMode` (`true`) or `b_DeCaffeineMode` (`false`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caffeinated: Option<bool>,
}
