use avian3d::prelude::RigidBody;
use bevy::{
    asset::{LoadState, RecursiveDependencyLoadState},
    prelude::*,
};
use openzt2_game_assets::{
    scene_prefab::ScenePrefabAsset,
    world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView,
};

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    staff_snapshot_types::StaffSnapshotRecord,
};
use crate::plugins::{
    economy::money_types::Money,
    staff::{
        staff_employment_types::{Employment, StaffRole},
        staff_presentation_variant_selection::{
            attach_staff_presentation_variant, staff_body_prefab, staff_presentation_variant,
            staff_role_for_definition,
        },
    },
    world_spawn::{
        persistent_id_types::{PersistentId, PersistentIdAllocator},
        prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance,
        world_membership_types::DefinitionId,
    },
};

/// Baseline world entities that staff identification treats as employees.
/// Saved staff replace them, as saved animals replace the baseline population.
pub(super) fn baseline_staff_entities(
    definitions: WorldDefinitionsView<'_>,
    defined_entities: &Query<(Entity, &DefinitionId)>,
) -> Vec<Entity> {
    defined_entities
        .iter()
        .filter(|(_, definition)| staff_role_for_definition(definitions, definition.0).is_some())
        .map(|(entity, _)| entity)
        .collect()
}

pub(super) fn resolve_staff_body_prefabs(
    records: &[StaffSnapshotRecord],
    definitions: WorldDefinitionsView<'_>,
    prefabs: &Assets<ScenePrefabAsset>,
    server: &AssetServer,
    retained_prefabs: &mut Vec<Handle<ScenePrefabAsset>>,
) -> Result<Option<Vec<Handle<ScenePrefabAsset>>>, WorldSnapshotPersistenceFailure> {
    let mut resolved = Vec::with_capacity(records.len());
    let mut ready = true;
    for record in records {
        let role = definitions
            .find_staff(record.role)
            .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
        let handle = staff_body_prefab(role, record.persistent_identifier, definitions)
            .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
        if matches!(
            server.get_load_state(handle.id()),
            Some(LoadState::Failed(_))
        ) || matches!(
            server.get_recursive_dependency_load_state(handle.id()),
            Some(RecursiveDependencyLoadState::Failed(_))
        ) {
            return Err(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier);
        }
        if !retained_prefabs.contains(&handle) {
            retained_prefabs.push(handle.clone());
        }
        ready &= prefabs.get(&handle).is_some();
        resolved.push(handle);
    }
    Ok(ready.then_some(resolved))
}

/// Spawns each saved employee's body with its saved employment. Staff
/// identification then attaches the role, look, name and work state.
pub(super) fn apply_staff_snapshot_records(
    commands: &mut Commands,
    records: &[StaffSnapshotRecord],
    resolved_prefabs: &[Handle<ScenePrefabAsset>],
    prefabs: &Assets<ScenePrefabAsset>,
    world_root: Entity,
    allocator: &mut PersistentIdAllocator,
    baseline_staff: &[Entity],
    definitions: WorldDefinitionsView<'_>,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    if records.len() != resolved_prefabs.len() {
        return Err(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier);
    }
    for record in records {
        allocator
            .reserve_imported(world_root, PersistentId(record.persistent_identifier))
            .map_err(|_| WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
    }
    for &entity in baseline_staff {
        commands.entity(entity).despawn();
    }
    for (record, handle) in records.iter().zip(resolved_prefabs) {
        let staff = spawn_loaded_scene_prefab_as_world_instance(
            commands,
            prefabs
                .get(handle)
                .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?,
            handle.clone(),
            world_root,
            record.definition,
            PersistentId(record.persistent_identifier),
            Transform::IDENTITY,
            true,
            None,
            RigidBody::Dynamic,
        );
        // The saved root pose already includes the authored root transform.
        commands.entity(staff).insert((
            Transform {
                translation: Vec3::from_array(record.translation),
                rotation: Quat::from_array(record.rotation),
                scale: Vec3::from_array(record.scale),
            },
            StaffRole(record.role),
            Employment {
                wage: Money(record.wage_cents_per_month),
                hired_tick: record.hired_tick,
                hired_month_ordinal: record.hired_month_ordinal,
            },
        ));
        // Dress the drawn look now, as hiring does: identification waits for
        // the simulation, while the body's animations attach straight away.
        if let Some((variant, _)) = definitions.find_staff(record.role).and_then(|role| {
            staff_presentation_variant(role, record.persistent_identifier, definitions)
        }) {
            attach_staff_presentation_variant(
                commands,
                staff,
                variant,
                record.persistent_identifier,
                definitions,
            );
        }
    }
    Ok(())
}
