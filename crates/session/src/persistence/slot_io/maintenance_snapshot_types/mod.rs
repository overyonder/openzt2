use openzt2_game_data::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LooseWasteSnapshotRecord {
    #[serde(default)]
    pub kind: LooseWasteSnapshotKind,
    pub persistent_identifier: u64,
    pub definition: AssetId,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
    pub units: u16,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum LooseWasteSnapshotKind {
    #[default]
    AnimalHabitat,
    Litter,
}

/// Worn condition, bin contents and animal waste-production deadlines,
/// restored onto the entity with the same persistent identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MaintainedObjectSnapshotRecord {
    pub persistent_identifier: u64,
    pub condition_permille: Option<u16>,
    /// Retained bin waste and the bin's capacity.
    pub contained_waste: Option<[u16; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_waste_production_tick: Option<u64>,
}

/// The Maintenance section. Saves before object state hold a bare waste array.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MaintenanceSnapshot {
    pub waste: Vec<LooseWasteSnapshotRecord>,
    pub objects: Vec<MaintainedObjectSnapshotRecord>,
}
