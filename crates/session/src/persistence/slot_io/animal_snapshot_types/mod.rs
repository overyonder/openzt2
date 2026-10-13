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
    pub pregnancy: Option<(u64, u64)>,
    pub crated: bool,
    pub release_restricted: bool,
    pub crating_restricted: bool,
    pub death: Option<(AssetId, u64)>,
    pub welfare: Option<AnimalWelfareSnapshotRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AnimalWelfareSnapshotRecord {
    pub needs: [i32; 11],
    pub triggers: u16,
    pub cumulative_points: [f32; 2],
    pub welfare: u16,
    pub habitat_suitability: [u16; 3],
    pub band: u8,
}
