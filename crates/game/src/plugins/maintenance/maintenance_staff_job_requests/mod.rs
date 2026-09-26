use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::staff::staff_job_types::StaffJob;
use crate::plugins::staff::staff_lifecycle_messages::StaffJobCancelled;
use crate::plugins::staff::staff_lifecycle_messages::StaffJobRequest;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

use super::maintenance_types::{
    AnimalHabitatWaste, AnimalHabitatWasteCleaningStaffJobKindMarker, FacilityContainedWaste,
    LooseLitterSweepingStaffJobKindMarker, LooseLitterWaste, MaintainableObjectConditionPermille,
    MaintenanceRepairStaffJobKindMarker, WasteContainerEmptyingStaffJobKindMarker,
};

pub(super) fn request_staff_repairs_for_damaged_maintainable_objects(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    changed_maintainable_objects: Query<
        (Entity, &DefinitionId, &MaintainableObjectConditionPermille),
        Changed<MaintainableObjectConditionPermille>,
    >,
    mut staff_job_requests: MessageWriter<StaffJobRequest>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (maintainable_object_entity, definition_id, maintainable_object_condition) in
        &changed_maintainable_objects
    {
        let Some(maintenance_definition) =
            world_definitions.find_maintenance_by_object(definition_id.0)
        else {
            continue;
        };
        if maintainable_object_condition.0 < maintenance_definition.repair_below_permille {
            // No authored staff task or request token exists for repair work;
            // the token stays unset until lowering evidence supplies one.
            staff_job_requests.write(StaffJobRequest {
                kind: StaffJobKind::Repair,
                target: maintainable_object_entity,
                urgency: 1_000_u16.saturating_sub(maintainable_object_condition.0),
                token: None,
            });
        }
    }
}

pub(super) fn request_staff_emptying_for_full_waste_containers(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    changed_waste_containers: Query<
        (Entity, &DefinitionId, &FacilityContainedWaste),
        Changed<FacilityContainedWaste>,
    >,
    mut staff_job_requests: MessageWriter<StaffJobRequest>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (waste_container_entity, definition_id, waste_container) in &changed_waste_containers {
        let Some(maintenance_definition) =
            world_definitions.find_maintenance_by_object(definition_id.0)
        else {
            continue;
        };
        let emptying_threshold_units = maintenance_definition.empty_at_units;
        if emptying_threshold_units != 0
            && waste_container.contained_waste_units >= emptying_threshold_units
        {
            // Controller tokens are not parsed yet; leave this request's token unset.
            let token = None;
            staff_job_requests.write(StaffJobRequest {
                kind: StaffJobKind::EmptyBin,
                target: waste_container_entity,
                urgency: calculate_waste_container_fullness_permille(waste_container),
                token,
            });
        }
    }
}

pub(super) fn request_staff_sweeping_for_spawned_litter(
    spawned_litter: Query<(Entity, &LooseLitterWaste), Added<LooseLitterWaste>>,
    mut staff_job_requests: MessageWriter<StaffJobRequest>,
) {
    for (litter_entity, loose_litter_waste) in &spawned_litter {
        if loose_litter_waste.uncontained_waste_units != 0 {
            staff_job_requests.write(StaffJobRequest {
                kind: StaffJobKind::SweepLitter,
                target: litter_entity,
                urgency: loose_litter_waste.uncontained_waste_units.min(1_000),
                token: Some(AssetId::from_key("t_sweeptrash")),
            });
        }
    }
}

pub(super) fn request_staff_cleaning_for_spawned_habitat_waste(
    spawned_habitat_waste: Query<(Entity, &AnimalHabitatWaste), Added<AnimalHabitatWaste>>,
    mut staff_job_requests: MessageWriter<StaffJobRequest>,
) {
    for (habitat_waste_entity, habitat_waste) in &spawned_habitat_waste {
        if habitat_waste.uncontained_waste_units != 0 {
            staff_job_requests.write(StaffJobRequest {
                kind: StaffJobKind::CleanHabitat,
                target: habitat_waste_entity,
                urgency: habitat_waste.uncontained_waste_units.min(1_000),
                token: Some(AssetId::from_key("t_rakepoo")),
            });
        }
    }
}

pub(super) fn requeue_cancelled_maintenance_staff_work_when_still_required(
    mut cancelled_staff_jobs: MessageReader<StaffJobCancelled>,
    maintainable_object_conditions: Query<(&DefinitionId, &MaintainableObjectConditionPermille)>,
    waste_containers: Query<(&DefinitionId, &FacilityContainedWaste)>,
    loose_litter_waste: Query<&LooseLitterWaste>,
    habitat_waste: Query<&AnimalHabitatWaste>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut staff_job_requests: MessageWriter<StaffJobRequest>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for cancelled_staff_job in cancelled_staff_jobs.read() {
        let current_urgency = match cancelled_staff_job.kind {
            StaffJobKind::Repair => maintainable_object_conditions
                .get(cancelled_staff_job.target)
                .ok()
                .and_then(|(definition_id, maintainable_object_condition)| {
                    let maintenance_definition =
                        world_definitions.find_maintenance_by_object(definition_id.0)?;
                    (maintainable_object_condition.0 < maintenance_definition.repair_below_permille)
                        .then_some(1_000_u16.saturating_sub(maintainable_object_condition.0))
                }),
            StaffJobKind::EmptyBin => waste_containers
                .get(cancelled_staff_job.target)
                .ok()
                .and_then(|(definition_id, waste_container)| {
                    let maintenance_definition =
                        world_definitions.find_maintenance_by_object(definition_id.0)?;
                    let emptying_threshold_units = maintenance_definition.empty_at_units;
                    (emptying_threshold_units != 0
                        && waste_container.contained_waste_units >= emptying_threshold_units)
                        .then_some(calculate_waste_container_fullness_permille(waste_container))
                }),
            StaffJobKind::SweepLitter => loose_litter_waste
                .get(cancelled_staff_job.target)
                .ok()
                .filter(|loose_litter_waste| loose_litter_waste.uncontained_waste_units != 0)
                .map(|loose_litter_waste| loose_litter_waste.uncontained_waste_units.min(1_000)),
            StaffJobKind::CleanHabitat => habitat_waste
                .get(cancelled_staff_job.target)
                .ok()
                .filter(|habitat_waste| habitat_waste.uncontained_waste_units != 0)
                .map(|habitat_waste| habitat_waste.uncontained_waste_units.min(1_000)),
            _ => None,
        };
        if let Some(current_urgency) = current_urgency {
            staff_job_requests.write(StaffJobRequest {
                kind: cancelled_staff_job.kind,
                target: cancelled_staff_job.target,
                urgency: current_urgency,
                token: cancelled_staff_job.token,
            });
        }
    }
}

pub(super) fn classify_spawned_staff_jobs_by_maintenance_work_kind(
    mut commands: Commands,
    spawned_staff_jobs: Query<(Entity, &StaffJob), Added<StaffJob>>,
) {
    for (staff_job_entity, staff_job) in &spawned_staff_jobs {
        match staff_job.kind {
            StaffJobKind::Repair => commands
                .entity(staff_job_entity)
                .insert(MaintenanceRepairStaffJobKindMarker),
            StaffJobKind::EmptyBin => commands
                .entity(staff_job_entity)
                .insert(WasteContainerEmptyingStaffJobKindMarker),
            StaffJobKind::SweepLitter => commands
                .entity(staff_job_entity)
                .insert(LooseLitterSweepingStaffJobKindMarker),
            StaffJobKind::CleanHabitat => commands
                .entity(staff_job_entity)
                .insert(AnimalHabitatWasteCleaningStaffJobKindMarker),
            _ => continue,
        };
    }
}

fn calculate_waste_container_fullness_permille(waste_container: &FacilityContainedWaste) -> u16 {
    if waste_container.contained_waste_capacity_units == 0 {
        return 0;
    }
    (u64::from(waste_container.contained_waste_units)
        .saturating_mul(1_000)
        .saturating_add(u64::from(waste_container.contained_waste_capacity_units) / 2)
        / u64::from(waste_container.contained_waste_capacity_units))
    .min(1_000) as u16
}
