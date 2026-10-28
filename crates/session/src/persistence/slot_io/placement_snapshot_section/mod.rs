use super::super::{
    persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    snapshot_container_types::WorldSnapshotSectionDirectoryEntry,
};
use avian3d::prelude::RigidBody;
use bevy::{
    asset::{LoadState, RecursiveDependencyLoadState},
    gltf::Gltf,
    prelude::*,
};
use openzt2_game_assets::{
    scene_prefab::ScenePrefabAsset,
    world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView,
};
use openzt2_game_data::{
    world_definitions::world_objects::{WorldObjectKind, WorldObjectPropertyFlags},
    AssetId,
};
use openzt2_world::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
use openzt2_world_state::world_spawn::{
    persistent_id_types::{PersistentId, PersistentIdAllocator},
    prefab_model_readiness::first_missing_prefab_collider_model_asset_id,
    world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::{
    habitat::habitat_types::HabitatLocatable,
    placement::{
        object_placement_definition_queries::resolve_object_placeable_definition,
        placed_object_types::PlacedObjectDefinitionReference,
    },
};
use std::collections::BTreeMap;

const MAXIMUM_PLACEMENT_RECORDS: usize = 65_536;
const RECORD_BYTES: usize = 64;
const HEADER: [u8; 4] = [1, 0, 0, 0];

#[derive(Debug, Clone, PartialEq)]
pub(super) struct PlacementSnapshotRecord {
    pub(super) persistent_identifier: u64,
    pub(super) definition: AssetId,
    pub(super) transform: Transform,
}

pub(super) type PlacementSnapshotEntities<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PersistentId,
        &'static DefinitionId,
        &'static Transform,
        &'static WorldMember,
        Has<PlacedObjectDefinitionReference>,
        Option<&'static ChildOf>,
    ),
    (
        Without<openzt2_zoo_state::maintenance::maintenance_types::AnimalHabitatWaste>,
        Without<openzt2_zoo_state::maintenance::maintenance_types::LooseLitterWaste>,
    ),
>;

fn placement_owns_definition(definitions: WorldDefinitionsView<'_>, id: AssetId) -> bool {
    definitions.find_object(id).is_some_and(|object| {
        !matches!(
            object.kind,
            WorldObjectKind::Animal
                | WorldObjectKind::Staff
                | WorldObjectKind::Guest
                | WorldObjectKind::Tank
                | WorldObjectKind::ShowStage
                | WorldObjectKind::Station
                | WorldObjectKind::Vehicle
                | WorldObjectKind::Fence
                | WorldObjectKind::Gate
                | WorldObjectKind::Path
        ) && !object
            .properties
            .contains_all(WorldObjectPropertyFlags::ANIMAL_WASTE)
            && resolve_object_placeable_definition(definitions, id).is_some()
    })
}

pub(super) fn capture_placement_records(
    entities: &PlacementSnapshotEntities,
    definitions: WorldDefinitionsView<'_>,
) -> Result<Vec<PlacementSnapshotRecord>, Failure> {
    let mut records: Vec<_> = entities
        .iter()
        .filter(|(_, _, definition, _, member, _, parent)| {
            parent.is_none_or(|parent| parent.parent() == member.root)
                && placement_owns_definition(definitions, definition.0)
        })
        .map(
            |(_, id, definition, transform, _, _, _)| PlacementSnapshotRecord {
                persistent_identifier: id.0,
                definition: definition.0,
                transform: *transform,
            },
        )
        .collect();
    records.sort_unstable_by_key(|record| record.persistent_identifier);
    validate_records(&records)?;
    Ok(records)
}

fn validate_records(records: &[PlacementSnapshotRecord]) -> Result<(), Failure> {
    if records.len() > MAXIMUM_PLACEMENT_RECORDS {
        return Err(Failure::CapacityExceeded);
    }
    for (index, record) in records.iter().enumerate() {
        if matches!(record.persistent_identifier, 0 | u64::MAX)
            || index > 0 && records[index - 1].persistent_identifier >= record.persistent_identifier
            || record.definition == AssetId::default()
            || !record.transform.translation.is_finite()
            || !record.transform.rotation.is_finite()
            || !record.transform.scale.is_finite()
            || record.transform.scale.min_element() <= 0.0
            || (record.transform.rotation.length_squared() - 1.0).abs() > 0.001
        {
            return Err(Failure::CorruptSnapshotSection);
        }
    }
    Ok(())
}

pub(super) fn encode_placement_records(
    records: &[PlacementSnapshotRecord],
) -> Result<Vec<u8>, Failure> {
    validate_records(records)?;
    // A header distinguishes a saved empty population from legacy baseline-only saves.
    let mut bytes = HEADER.to_vec();
    for record in records {
        bytes.extend_from_slice(&record.persistent_identifier.to_le_bytes());
        bytes.extend_from_slice(&record.definition.0);
        for value in record
            .transform
            .translation
            .to_array()
            .into_iter()
            .chain(record.transform.rotation.to_array())
            .chain(record.transform.scale.to_array())
        {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    Ok(bytes)
}

pub(super) fn decode_placement_records(
    bytes: &[u8],
    section: WorldSnapshotSectionDirectoryEntry,
) -> Result<Option<Vec<PlacementSnapshotRecord>>, Failure> {
    let count = section.encoded_record_count as usize;
    if count > MAXIMUM_PLACEMENT_RECORDS {
        return Err(Failure::CapacityExceeded);
    }
    let payload = &bytes[section.payload_byte_range_within_container(bytes.len())?];
    if payload.is_empty() && count == 0 {
        return Ok(None);
    }
    if payload.len() != HEADER.len() + count * RECORD_BYTES || !payload.starts_with(&HEADER) {
        return Err(Failure::CorruptSnapshotSection);
    }
    let mut records = Vec::with_capacity(count);
    for row in payload[HEADER.len()..].chunks_exact(RECORD_BYTES) {
        let floats: Vec<_> = row[24..]
            .chunks_exact(4)
            .map(|value| {
                f32::from_le_bytes(value.try_into().expect("fixed-width placement scalar"))
            })
            .collect();
        records.push(PlacementSnapshotRecord {
            persistent_identifier: u64::from_le_bytes(
                row[..8].try_into().expect("fixed-width identity"),
            ),
            definition: AssetId(row[8..24].try_into().expect("fixed-width asset identity")),
            transform: Transform {
                translation: Vec3::new(floats[0], floats[1], floats[2]),
                rotation: Quat::from_xyzw(floats[3], floats[4], floats[5], floats[6]),
                scale: Vec3::new(floats[7], floats[8], floats[9]),
            },
        });
    }
    validate_records(&records)?;
    Ok(Some(records))
}

pub(super) fn baseline_placement_entities(
    entities: &PlacementSnapshotEntities,
    definitions: WorldDefinitionsView<'_>,
    root: Entity,
) -> Vec<Entity> {
    entities
        .iter()
        .filter(|(_, _, definition, _, member, _, parent)| {
            member.root == root
                && parent.is_none_or(|parent| parent.parent() == root)
                && placement_owns_definition(definitions, definition.0)
        })
        .map(|(entity, ..)| entity)
        .collect()
}

pub(super) fn resolve_placement_prefabs(
    records: &[PlacementSnapshotRecord],
    definitions: WorldDefinitionsView<'_>,
    prefabs: &Assets<ScenePrefabAsset>,
    models: &Assets<Gltf>,
    server: &AssetServer,
    retained: &mut Vec<Handle<ScenePrefabAsset>>,
) -> Result<Option<Vec<Handle<ScenePrefabAsset>>>, Failure> {
    let mut ready = true;
    let mut resolved = Vec::with_capacity(records.len());
    for record in records {
        if !placement_owns_definition(definitions, record.definition) {
            bevy::log::warn!(?record, "saved placement definition is unavailable");
            return Err(Failure::UnknownAssetIdentifier);
        }
        // Saved scenery can retain tilted or freely rotated authored poses.
        // Purchase-grid restrictions do not constrain restored world poses.
        let object = definitions
            .find_object(record.definition)
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let handle = definitions
            .scene(
                openzt2_zoo_state::placement::object_placement_definition_queries::object_placement_prefab_identifier(object)
                    .ok_or(Failure::UnknownAssetIdentifier)?,
            )
            .ok_or_else(|| {
                bevy::log::warn!(?record, prefab = ?object.prefab, "saved placement prefab is unavailable");
                Failure::UnknownAssetIdentifier
            })?;
        if matches!(
            server.get_load_state(handle.id()),
            Some(LoadState::Failed(_))
        ) || matches!(
            server.get_recursive_dependency_load_state(handle.id()),
            Some(RecursiveDependencyLoadState::Failed(_))
        ) {
            bevy::log::warn!(?record, ?handle, "saved placement prefab failed to load");
            return Err(Failure::UnknownAssetIdentifier);
        }
        if !retained.contains(&handle) {
            retained.push(handle.clone());
        }
        ready &= prefabs.get(&handle).is_some_and(|prefab| {
            first_missing_prefab_collider_model_asset_id(prefab, models).is_none()
        });
        resolved.push(handle);
    }
    Ok(ready.then_some(resolved))
}

pub(super) fn placement_hydration_complete(
    definitions: WorldDefinitionsView<'_>,
    definition: AssetId,
    transform: &Transform,
    has_grid_placement: bool,
    has_donation_dock: bool,
) -> bool {
    let Some(placeable) = resolve_object_placeable_definition(definitions, definition) else {
        return false;
    };
    let Some(object) = definitions.find_object(definition) else {
        return false;
    };
    let grid_pose = openzt2_zoo_state::placement::object_placement_validation::calculate_authored_object_placement_eighth_turns(placeable, transform)
        .and_then(|turns| openzt2_zoo_state::placement::object_placement_validation::calculate_object_placement_footprint_origin(placeable, transform, turns));
    // Canonical placement hydration projects only authored grid orientations.
    (grid_pose.is_none() || has_grid_placement)
        && (!object
            .properties
            .contains_all(WorldObjectPropertyFlags::DONATION_ACCEPTOR)
            || has_donation_dock)
}

pub(super) fn apply_placement_records(
    commands: &mut Commands,
    records: &[PlacementSnapshotRecord],
    prefabs: &Assets<ScenePrefabAsset>,
    resolved: &[Handle<ScenePrefabAsset>],
    definitions: WorldDefinitionsView<'_>,
    baseline: &[Entity],
    root: Entity,
    allocator: &mut PersistentIdAllocator,
) -> Result<BTreeMap<u64, Entity>, Failure> {
    if records.len() != resolved.len() {
        return Err(Failure::CorruptSnapshotSection);
    }
    validate_records(records)?;
    let spawning_inputs = records
        .iter()
        .zip(resolved)
        .map(|(record, handle)| {
            let definition = resolve_object_placeable_definition(definitions, record.definition)
                .filter(|_| placement_owns_definition(definitions, record.definition))
                .ok_or(Failure::UnknownAssetIdentifier)?;
            let prefab = prefabs.get(handle).ok_or(Failure::UnknownAssetIdentifier)?;
            Ok((definition.moving_footprint, prefab))
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    let mut restored = BTreeMap::new();
    for record in records {
        allocator
            .reserve_imported(root, PersistentId(record.persistent_identifier))
            .map_err(|_| Failure::BrokenPersistentEntityReference)?;
    }
    // Replace this cohort, including baseline objects deleted before the save.
    for entity in baseline {
        commands.entity(*entity).despawn();
    }
    for ((record, handle), (moving_footprint, prefab)) in
        records.iter().zip(resolved).zip(spawning_inputs)
    {
        let entity = spawn_loaded_scene_prefab_as_world_instance(
            commands,
            prefab,
            handle.clone(),
            root,
            record.definition,
            PersistentId(record.persistent_identifier),
            Transform::IDENTITY,
            true,
            None,
            if moving_footprint {
                RigidBody::Dynamic
            } else {
                RigidBody::Static
            },
        );
        // Saved root poses already contain the prefab's authored root transform.
        commands
            .entity(entity)
            .insert((record.transform, HabitatLocatable));
        restored.insert(record.persistent_identifier, entity);
    }
    Ok(restored)
}

pub(super) fn capture_world_placement_records(
    queries: &super::world_snapshot_capture_system_parameters::WorldSnapshotCaptureQueries,
) -> Result<Vec<PlacementSnapshotRecord>, Failure> {
    let definitions = queries
        .active_world_definitions
        .get(&queries.world_definition_assets)
        .ok_or(Failure::UnknownAssetIdentifier)?;
    capture_placement_records(&queries.placements, definitions)
}

#[cfg(test)]
mod tests;
