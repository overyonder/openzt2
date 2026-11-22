use bevy::{
    ecs::{component::Mutable, query::QueryData, system::EntityCommands},
    prelude::*,
};

use openzt2_world_state::world_spawn::persistent_id_index::PersistentIdIndex;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::{
    PendingTankRestoration, Tank, TankEnvironment, TankGeometry, TankWater, WaterQuality,
};

#[derive(QueryData)]
#[query_data(mutable)]
pub(super) struct RestoredTankComponents {
    tank: Option<&'static mut Tank>,
    geometry: Option<&'static mut TankGeometry>,
    water: Option<&'static mut TankWater>,
    environment: Option<&'static mut TankEnvironment>,
    water_quality: Option<&'static mut WaterQuality>,
}

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    tank_snapshot_types::{LoadedTankSnapshotIdentity, LoadedTankSnapshotRecord},
};

/// Seeded records wait for the aquatic plugin to find the habitat their walls
/// close; older records restore onto the entity with their persistent identifier.
pub(super) fn apply_tank_snapshot_records_to_live_world(
    commands: &mut Commands,
    tank_snapshot_records: Vec<LoadedTankSnapshotRecord>,
    persistent_ids: &PersistentIdIndex,
    tanks: &mut Query<RestoredTankComponents>,
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
                let tank_entity = persistent_ids
                    .get(world_root_entity, persistent_identifier)
                    .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
                let state = tanks.get_mut(tank_entity).map_err(|_| {
                    WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference
                })?;
                if tank_snapshot_record.water_limit_missing {
                    commands.entity(tank_entity).insert(openzt2_zoo_state::aquatic::aquatic_simulation_types::TankWaterLimitNeedsInitialization);
                }
                let mut entity = commands.entity(tank_entity);
                restore_tank_component(
                    &mut entity,
                    state.tank,
                    Tank {
                        definition: tank_snapshot_record.tank_definition_identifier,
                    },
                );
                restore_tank_component(&mut entity, state.geometry, tank_snapshot_record.geometry);
                restore_tank_component(&mut entity, state.water, TankWater::default());
                restore_tank_component(&mut entity, state.environment, TankEnvironment::default());
                restore_tank_component(
                    &mut entity,
                    state.water_quality,
                    tank_snapshot_record.water_quality,
                );
            }
        }
    }

    Ok(())
}

fn restore_tank_component<T: Component<Mutability = Mutable> + PartialEq>(
    entity: &mut EntityCommands,
    current: Option<Mut<T>>,
    value: T,
) {
    if let Some(mut current) = current {
        current.set_if_neq(value);
    } else {
        entity.insert(value);
    }
}
