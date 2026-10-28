use bevy::math::IVec2;
use openzt2_game_data::AssetId;

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::TankGeometry;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterQuality;

/// A live tank as saved: identified by its habitat's seed cell.
#[derive(Debug, Clone, Copy)]
pub(super) struct TankSnapshotRecord {
    pub seed_cell: IVec2,
    pub tank_definition_identifier: AssetId,
    pub geometry: TankGeometry,
    pub water_quality: WaterQuality,
}

/// How a loaded tank record finds its live habitat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LoadedTankSnapshotIdentity {
    HabitatSeedCell(IVec2),
    /// Older saves: an existing entity carrying this persistent identifier.
    LegacyPersistentEntity(PersistentId),
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LoadedTankSnapshotRecord {
    pub identity: LoadedTankSnapshotIdentity,
    pub water_limit_missing: bool,
    pub tank_definition_identifier: AssetId,
    pub geometry: TankGeometry,
    pub water_quality: WaterQuality,
}
