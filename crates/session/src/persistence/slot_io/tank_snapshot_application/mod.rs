use bevy::prelude::{Commands, Entity, Query};

use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::{
    PendingTankRestoration, Tank, TankEnvironment, TankWater,
};

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    tank_snapshot_types::{LoadedTankSnapshotIdentity, LoadedTankSnapshotRecord},
};

/// Seeded records wait for the aquatic plugin to find the habitat their walls
/// close; older records restore onto the entity with their persistent identifier.
pub(super) fn apply_tank_snapshot_records_to_live_world(
    commands: &mut Commands,
    tank_snapshot_records: Vec<LoadedTankSnapshotRecord>,
    entities_with_persistent_identifiers: &Query<(Entity, &PersistentId)>,
    world_root_entity: Entity,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    for tank_snapshot_record in tank_snapshot_records {
        match tank_snapshot_record.identity {
            LoadedTankSnapshotIdentity::HabitatSeedCell(seed_cell) => {
                commands.spawn((
                    PendingTankRestoration {
                        seed_cell,
                        definition: tank_snapshot_record.tank_definition_identifier,
                        geometry: tank_snapshot_record.geometry,
                        water_quality: tank_snapshot_record.water_quality,
                        water_limit_missing: tank_snapshot_record.water_limit_missing,
                    },
                    WorldMember {
                        root: world_root_entity,
                    },
                ));
            }
            LoadedTankSnapshotIdentity::LegacyPersistentEntity(persistent_identifier) => {
                let tank_entity = entities_with_persistent_identifiers
                    .iter()
                    .find_map(|(entity, candidate)| {
                        (*candidate == persistent_identifier).then_some(entity)
                    })
                    .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
                if tank_snapshot_record.water_limit_missing {
                    commands.entity(tank_entity).insert(openzt2_zoo_state::aquatic::aquatic_simulation_types::TankWaterLimitNeedsInitialization);
                }
                commands.entity(tank_entity).insert((
                    Tank {
                        definition: tank_snapshot_record.tank_definition_identifier,
                    },
                    tank_snapshot_record.geometry,
                    TankWater::default(),
                    TankEnvironment::default(),
                    tank_snapshot_record.water_quality,
                ));
            }
        }
    }

    Ok(())
}
