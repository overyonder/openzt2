use openzt2_game_data::AssetId;

use crate::plugins::{
    aquatic::aquatic_simulation_types::{TankGeometry, WaterQuality},
    world_spawn::persistent_id_types::PersistentId,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct TankSnapshotRecord {
    pub persistent_identifier: PersistentId,
    pub tank_definition_identifier: AssetId,
    pub geometry: TankGeometry,
    pub water_quality: WaterQuality,
}
