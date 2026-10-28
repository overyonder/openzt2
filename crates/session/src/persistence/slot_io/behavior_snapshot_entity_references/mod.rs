use bevy::{ecs::query::QueryData, prelude::*};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::{
    habitat::habitat_types::HabitatRegion,
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

/// Reads only the components needed by behavior persistence, allowing loading
/// to mutate its resources and unrelated live components in the same system.
#[derive(QueryData)]
pub(in crate::persistence) struct BehaviorSnapshotEntityData {
    pub(super) entity: Entity,
    pub(super) animal_terrain_water_contact: Option<&'static openzt2_zoo_state::locomotion::locomotion_types::AnimalTerrainWaterContact>,
    pub(super) behavior_random_stream: Option<&'static openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream>,
    pub(super) behavior_self_token: Option<&'static openzt2_zoo_state::animal_behavior::behavior_token_types::BehaviorSelfToken>,
    pub(super) behavior_task_effective_score: Option<&'static openzt2_zoo_state::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore>,
    pub(super) behavior_task_execution_state: Option<&'static openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState>,
    pub(super) child_of: Option<&'static ChildOf>,
    pub(super) contact_steering: Option<&'static openzt2_zoo_state::locomotion::locomotion_types::ContactSteering>,
    pub(super) current_job: Option<&'static openzt2_zoo_state::staff::staff_job_types::CurrentJob>,
    pub(super) definition_id: Option<&'static openzt2_world_state::world_spawn::world_membership_types::DefinitionId>,
    pub(super) destination: Option<&'static openzt2_zoo_state::locomotion::locomotion_types::Destination>,
    pub(super) docking: Option<&'static openzt2_zoo_state::locomotion::locomotion_types::Docking>,
    pub(super) entity_pursuit: Option<&'static openzt2_zoo_state::locomotion::locomotion_types::EntityPursuit>,
    pub(super) global_transform: Option<&'static GlobalTransform>,
    pub(super) habitat_region: Option<&'static HabitatRegion>,
    pub(super) job_claim: Option<&'static openzt2_zoo_state::staff::staff_job_types::JobClaim>,
    pub(super) job_progress: Option<&'static openzt2_zoo_state::staff::staff_job_types::JobProgress>,
    pub(super) linear_velocity: Option<&'static avian3d::prelude::LinearVelocity>,
    pub(super) locomotion_mode: Option<&'static openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode>,
    pub(super) motion_progress: Option<&'static openzt2_zoo_state::locomotion::locomotion_types::MotionProgress>,
    pub(super) nav_agent: Option<&'static openzt2_zoo_state::locomotion::locomotion_types::NavAgent>,
    pub(super) path_tile: Option<&'static PathTile>,
    pub(super) pending_behavior_docking_completion: Option<&'static openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorDockingCompletion>,
    pub(super) pending_behavior_move_completion: Option<&'static openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorMoveCompletion>,
    pub(super) pending_behavior_pursuit: Option<&'static openzt2_zoo_state::animal_behavior::behavior_pursuit_types::PendingBehaviorPursuit>,
    pub(super) pending_behavior_task_failure: Option<&'static openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure>,
    pub(super) persistent_id: Option<&'static PersistentId>,
    pub(super) rigid_body: Option<&'static avian3d::prelude::RigidBody>,
    pub(super) route: Option<&'static openzt2_zoo_state::locomotion::locomotion_types::Route>,
    pub(super) selected_natural_water_target: Option<&'static openzt2_zoo_state::animal_behavior::behavior_natural_water_target_types::SelectedNaturalWaterTarget>,
    pub(super) staff_job: Option<&'static openzt2_zoo_state::staff::staff_job_types::StaffJob>,
    pub(super) staff_job_behavior_task: Option<&'static openzt2_zoo_state::staff::staff_job_types::StaffJobBehaviorTask>,
    pub(super) swim_layer: Option<&'static openzt2_zoo_state::locomotion::locomotion_types::SwimLayer>,
    pub(super) synchronized_behavior_owner: Option<&'static openzt2_zoo_state::animal_behavior::behavior_synchronized_set_types::SynchronizedBehaviorOwner>,
    pub(super) tank_cleaning_request_site: Option<&'static openzt2_zoo_state::staff::staff_job_types::TankCleaningRequestSite>,
    pub(super) terrain_water_region_source: Option<&'static TerrainWaterRegionSource>,
    pub(super) velocity: Option<&'static openzt2_zoo_state::locomotion::locomotion_types::Velocity>,
    pub(super) world_member: Option<&'static openzt2_world_state::world_spawn::world_membership_types::WorldMember>,
}

pub(super) type BehaviorSnapshotEntity<'w, 's> = BehaviorSnapshotEntityDataItem<'w, 's>;
pub(in crate::persistence) type BehaviorSnapshotEntities<'w, 's> =
    Query<'w, 's, BehaviorSnapshotEntityData>;

pub(super) fn capture_behavior_entity_reference(
    entity: Entity,
    entities: &BehaviorSnapshotEntities,
) -> Result<BehaviorSnapshotEntityReference, WorldSnapshotPersistenceFailure> {
    let entity = entities
        .get(entity)
        .map_err(|_| WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
    if let Some(path) = entity.path_tile {
        return Ok(BehaviorSnapshotEntityReference::PathTile(
            path.cell.to_array(),
        ));
    }
    if let Some(water) = entity.terrain_water_region_source {
        let path = water
            .terrain()
            .path()
            .ok_or(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier)?;
        return Ok(BehaviorSnapshotEntityReference::TerrainWater {
            terrain: path.to_string(),
            source_region: water.source_region_index(),
        });
    }
    if let Some(region) = entity.habitat_region {
        return habitat_seed(region).map(BehaviorSnapshotEntityReference::Habitat);
    }
    if let Some(site) = entity.tank_cleaning_request_site {
        let region = entities
            .get(site.tank)
            .ok()
            .and_then(|tank| tank.habitat_region)
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        let position = entity
            .global_transform
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?
            .translation();
        return Ok(BehaviorSnapshotEntityReference::TankCleaningSite {
            tank: habitat_seed(region)?,
            position: position.to_array().map(f32::to_bits),
        });
    }
    if let Some(token) = entity.behavior_self_token {
        let recipient = entity
            .child_of
            .and_then(|parent| entities.get(parent.parent()).ok())
            .and_then(|recipient| recipient.persistent_id.copied())
            .ok_or(WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
        return Ok(BehaviorSnapshotEntityReference::SelfToken {
            recipient: recipient.0,
            name: token.name,
            available_at: token.available_at,
            expires_at: token.expires_at,
        });
    }
    if let Some(id) = entity.persistent_id {
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
        if !entity.persistent_id.is_some()
            && !entity.path_tile.is_some()
            && !entity.terrain_water_region_source.is_some()
            && !entity.habitat_region.is_some()
            && !entity.tank_cleaning_request_site.is_some()
        {
            continue;
        }
        let reference = capture_behavior_entity_reference(entity.entity, entities)?;
        if index.insert(reference, entity.entity).is_some() {
            return Err(WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier);
        }
        if let Some(id) = entity.persistent_id {
            let alias = BehaviorSnapshotEntityReference::Persistent(id.0);
            if index
                .get(&alias)
                .is_some_and(|candidate| *candidate != entity.entity)
            {
                return Err(WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier);
            }
            index.insert(alias, entity.entity);
        }
        if let Some(region) = entity.habitat_region {
            let alias = BehaviorSnapshotEntityReference::Habitat(habitat_seed(region)?);
            if index
                .get(&alias)
                .is_some_and(|candidate| *candidate != entity.entity)
            {
                return Err(WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier);
            }
            index.insert(alias, entity.entity);
        }
    }
    Ok(index)
}
