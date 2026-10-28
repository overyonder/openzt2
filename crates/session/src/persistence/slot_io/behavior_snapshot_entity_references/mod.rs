use bevy::{
    ecs::{resource::IsResource, world::EntityRefExcept},
    prelude::*,
};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::{
    animal_behavior::behavior_token_types::BehaviorSelfToken,
    habitat::habitat_types::HabitatRegion, staff::staff_job_types::TankCleaningRequestSite,
    terrain::terrain_water_presentation_types::TerrainWaterRegionSource,
    topology::topology_edit_types::TopologySnapshot, topology::topology_graph_types::PathTile,
};
use std::collections::BTreeMap;

pub(super) fn planned_behavior_topology_references(
    records: &[TopologySnapshot],
) -> Result<Vec<BehaviorSnapshotEntityReference>, WorldSnapshotPersistenceFailure> {
    use openzt2_navigation::habitat::habitat_region_derivation::{
        derive_enclosed_habitat_regions_from_fence_segments, HabitatFenceSegment,
    };
    let nodes = records
        .iter()
        .filter_map(|record| match record {
            TopologySnapshot::Node { id, cell } => Some((id.0, cell.truncate())),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let segments = records
        .iter()
        .filter_map(|record| match record {
            TopologySnapshot::Fence { a, b, .. } => Some((a, b)),
            _ => None,
        })
        .map(|(a, b)| {
            Ok(HabitatFenceSegment {
                fence_entity: Entity::PLACEHOLDER,
                first_topology_node: *nodes
                    .get(&a.0)
                    .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?,
                second_topology_node: *nodes
                    .get(&b.0)
                    .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?,
                boundary_is_breached: false,
            })
        })
        .collect::<Result<Vec<_>, WorldSnapshotPersistenceFailure>>()?;
    let mut references = records
        .iter()
        .filter_map(|record| match record {
            TopologySnapshot::Path { cell, .. } => {
                Some(BehaviorSnapshotEntityReference::PathTile(cell.to_array()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if let Some(first) = nodes.values().next().copied() {
        let (minimum, maximum) = nodes
            .values()
            .fold((first, first), |(minimum, maximum), cell| {
                (minimum.min(*cell), maximum.max(*cell))
            });
        references.extend(
            derive_enclosed_habitat_regions_from_fence_segments(
                &segments,
                IRect::from_corners(minimum, maximum),
            )
            .iter()
            .filter_map(|region| {
                region
                    .topology_cells
                    .first()
                    .map(|cell| BehaviorSnapshotEntityReference::Habitat(cell.to_array()))
            }),
        );
    }
    Ok(references)
}

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    behavior_snapshot_types::BehaviorSnapshotEntityReference,
};

// The load coordinator mutates these unrelated components while resolving tasks.
pub(super) type BehaviorSnapshotEntity<'w, 's> = EntityRefExcept<
    'w,
    's,
    (
        super::pending_snapshot_application_types::PendingSnapshotApplication,
        openzt2_zoo_state::shows::show_stage_types::ShowStageOpenState,
        openzt2_zoo_state::shows::show_stage_types::ShowName,
        openzt2_interface_state::photos::photo_challenge_types::PhotoChallengeProgress,
    ),
>;

pub(in crate::persistence) type BehaviorSnapshotEntities<'w, 's> =
    Query<'w, 's, BehaviorSnapshotEntity<'static, 'static>, Without<IsResource>>;

pub(super) fn capture_behavior_entity_reference(
    entity: Entity,
    entities: &BehaviorSnapshotEntities,
) -> Result<BehaviorSnapshotEntityReference, WorldSnapshotPersistenceFailure> {
    let entity = entities
        .get(entity)
        .map_err(|_| WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
    if let Some(path) = entity.get::<PathTile>() {
        return Ok(BehaviorSnapshotEntityReference::PathTile(
            path.cell.to_array(),
        ));
    }
    if let Some(water) = entity.get::<TerrainWaterRegionSource>() {
        let path = water
            .terrain()
            .path()
            .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
        return Ok(BehaviorSnapshotEntityReference::TerrainWater {
            terrain: path.to_string(),
            source_region: water.source_region_index(),
        });
    }
    if let Some(region) = entity.get::<HabitatRegion>() {
        return habitat_seed(region).map(BehaviorSnapshotEntityReference::Habitat);
    }
    if let Some(site) = entity.get::<TankCleaningRequestSite>() {
        let region = entities
            .get(site.tank)
            .ok()
            .and_then(|tank| tank.get::<HabitatRegion>())
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        let position = entity
            .get::<GlobalTransform>()
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?
            .translation();
        return Ok(BehaviorSnapshotEntityReference::TankCleaningSite {
            tank: habitat_seed(region)?,
            position: position.to_array().map(f32::to_bits),
        });
    }
    if let Some(token) = entity.get::<BehaviorSelfToken>() {
        let recipient = entity
            .get::<ChildOf>()
            .and_then(|parent| entities.get(parent.parent()).ok())
            .and_then(|recipient| recipient.get::<PersistentId>().copied())
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        return Ok(BehaviorSnapshotEntityReference::SelfToken {
            recipient: recipient.0,
            name: token.name,
            available_at: token.available_at,
            expires_at: token.expires_at,
        });
    }
    if let Some(id) = entity.get::<PersistentId>() {
        return Ok(BehaviorSnapshotEntityReference::Persistent(id.0));
    }
    Err(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)
}

fn habitat_seed(region: &HabitatRegion) -> Result<[i32; 2], WorldSnapshotPersistenceFailure> {
    openzt2_zoo_state::aquatic::aquatic_simulation_types::tank_habitat_seed_cell(region)
        .map(|cell| cell.to_array())
        .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)
}

pub(super) fn index_behavior_snapshot_entity_references(
    entities: &BehaviorSnapshotEntities,
) -> Result<BTreeMap<BehaviorSnapshotEntityReference, Entity>, WorldSnapshotPersistenceFailure> {
    let mut index = BTreeMap::new();
    for entity in entities {
        // Only identity-bearing entities participate; unrelated presentation
        // children do not require persistent identities.
        if !entity.contains::<PersistentId>()
            && !entity.contains::<PathTile>()
            && !entity.contains::<TerrainWaterRegionSource>()
            && !entity.contains::<HabitatRegion>()
            && !entity.contains::<TankCleaningRequestSite>()
        {
            continue;
        }
        let reference = capture_behavior_entity_reference(entity.id(), entities)?;
        if index.insert(reference, entity.id()).is_some() {
            return Err(WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier);
        }
        if let Some(id) = entity.get::<PersistentId>() {
            let alias = BehaviorSnapshotEntityReference::Persistent(id.0);
            if index
                .get(&alias)
                .is_some_and(|candidate| *candidate != entity.id())
            {
                return Err(WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier);
            }
            index.insert(alias, entity.id());
        }
        if let Some(region) = entity.get::<HabitatRegion>() {
            let alias = BehaviorSnapshotEntityReference::Habitat(habitat_seed(region)?);
            if index
                .get(&alias)
                .is_some_and(|candidate| *candidate != entity.id())
            {
                return Err(WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier);
            }
            index.insert(alias, entity.id());
        }
    }
    Ok(index)
}
