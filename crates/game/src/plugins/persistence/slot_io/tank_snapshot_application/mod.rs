use bevy::prelude::{Commands, Entity, Query};

use crate::plugins::{
    aquatic::aquatic_simulation_types::{HydratedTankSurface, Tank},
    world_spawn::persistent_id_types::PersistentId,
};

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    tank_snapshot_types::TankSnapshotRecord,
};

pub(super) fn apply_tank_snapshot_records_to_existing_tank_entities(
    commands: &mut Commands,
    tank_snapshot_records: Vec<TankSnapshotRecord>,
    entities_with_persistent_identifiers: &Query<(Entity, &PersistentId)>,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    for tank_snapshot_record in tank_snapshot_records {
        let tank_entity = entities_with_persistent_identifiers
            .iter()
            .find_map(|(entity, persistent_identifier)| {
                (*persistent_identifier == tank_snapshot_record.persistent_identifier)
                    .then_some(entity)
            })
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;

        commands.entity(tank_entity).insert((
            Tank {
                definition: tank_snapshot_record.tank_definition_identifier,
            },
            HydratedTankSurface::restored(tank_snapshot_record.geometry),
            tank_snapshot_record.water_quality,
        ));
    }

    Ok(())
}
