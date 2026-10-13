use openzt2_game_data::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HabitatWasteSnapshotRecord {
    pub persistent_identifier: u64,
    pub definition: AssetId,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub units: u16,
}
