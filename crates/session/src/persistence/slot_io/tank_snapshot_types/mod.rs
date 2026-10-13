use openzt2_game_data::AssetId;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::TankGeometry;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterQuality;

#[derive(Debug, Clone, Copy)]
pub(super) struct TankSnapshotRecord {
    pub persistent_identifier: PersistentId,
    pub tank_definition_identifier: AssetId,
    pub geometry: TankGeometry,
    pub water_quality: WaterQuality,
}
