use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::{
    StaffJobKind, StaffRequestControllerDefinition, StaffRequestThresholdComparison,
    StaffRequestThresholdValue,
};
use openzt2_game_data::AssetId;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::staff::staff_job_types::{authored_staff_request_urgency, StaffJob};
use openzt2_zoo_state::staff::staff_lifecycle_messages::StaffJobCancelled;
use openzt2_zoo_state::staff::staff_lifecycle_messages::StaffJobRequest;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

use super::maintenance_types::{
    AnimalHabitatWaste, FacilityContainedWaste, LooseLitterSweepingStaffJobKindMarker,
    LooseLitterWaste, WasteContainerEmptyingStaffJobKindMarker,
};

/// The authored urgency of `token` on the target's own staff-request
/// controller. Staff serve every request in the order of its authored
/// priority, so maintenance work without an authored request is not raised.
fn authored_request_urgency(
    world_definitions: openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView<'_>,
    definition: AssetId,
    token: AssetId,
) -> Option<u32> {
    world_definitions
        .staff_request_rows(definition)?
        .iter()
        .find(|row| row.request.token == Some(token))
        .and_then(|row| authored_staff_request_urgency(row.request.priority))
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
        if emptying_threshold_units == 0
            || waste_container.contained_waste_units < emptying_threshold_units
        {
            continue;
        }
        for token in [
            AssetId::from_key("t_emptytrash"),
            AssetId::from_key("t_emptyrecyclingbin"),
        ] {
            if let Some(urgency) =
                authored_request_urgency(world_definitions, definition_id.0, token)
            {
                staff_job_requests.write(StaffJobRequest {
                    kind: StaffJobKind::EmptyBin,
                    target: waste_container_entity,
                    urgency,
                    token: Some(token),
                });
            }
        }
    }
}

pub(super) fn request_staff_sweeping_for_spawned_litter(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    spawned_litter: Query<(Entity, &DefinitionId, &LooseLitterWaste), Added<LooseLitterWaste>>,
    mut staff_job_requests: MessageWriter<StaffJobRequest>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let token = AssetId::from_key("t_sweeptrash");
    for (litter_entity, definition_id, loose_litter_waste) in &spawned_litter {
        if loose_litter_waste.uncontained_waste_units == 0 {
            continue;
        }
        if let Some(urgency) = authored_request_urgency(world_definitions, definition_id.0, token) {
            staff_job_requests.write(StaffJobRequest {
                kind: StaffJobKind::SweepLitter,
                target: litter_entity,
                urgency,
                token: Some(token),
            });
        }
    }
}

pub(super) fn request_staff_cleaning_for_spawned_habitat_waste(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    spawned_habitat_waste: Query<
        (Entity, &DefinitionId, &AnimalHabitatWaste),
        Added<AnimalHabitatWaste>,
    >,
    mut staff_job_requests: MessageWriter<StaffJobRequest>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let token = AssetId::from_key("t_rakepoo");
    for (habitat_waste_entity, definition_id, habitat_waste) in &spawned_habitat_waste {
        if habitat_waste.uncontained_waste_units == 0 {
            continue;
        }
        if let Some(urgency) = authored_request_urgency(world_definitions, definition_id.0, token) {
            staff_job_requests.write(StaffJobRequest {
                kind: StaffJobKind::CleanHabitat,
                target: habitat_waste_entity,
                urgency,
                token: Some(token),
            });
        }
    }
}

/// A cancelled request whose condition still holds is raised again at the
/// authored priority of the controller that raised it.
pub(super) fn requeue_cancelled_maintenance_staff_work_when_still_required(
    mut cancelled_staff_jobs: MessageReader<StaffJobCancelled>,
    definitions: Query<&DefinitionId>,
    waste_containers: Query<&FacilityContainedWaste>,
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
        let Some(token) = cancelled_staff_job.token else {
            continue;
        };
        let target = cancelled_staff_job.target;
        let Some(row) = definitions
            .get(target)
            .ok()
            .and_then(|definition| world_definitions.staff_request_rows(definition.0))
            .and_then(|rows| rows.iter().find(|row| row.request.token == Some(token)))
        else {
            continue;
        };
        let Some(urgency) = authored_staff_request_urgency(row.request.priority) else {
            continue;
        };
        let still_required = match cancelled_staff_job.kind {
            StaffJobKind::EmptyBin => waste_containers.get(target).is_ok_and(|waste| {
                !authored_cancel_rule_holds(row, f32::from(waste.contained_waste_units))
            }),
            StaffJobKind::SweepLitter => loose_litter_waste
                .get(target)
                .is_ok_and(|litter| litter.uncontained_waste_units != 0),
            StaffJobKind::CleanHabitat => habitat_waste
                .get(target)
                .is_ok_and(|waste| waste.uncontained_waste_units != 0),
            _ => false,
        };
        if still_required {
            staff_job_requests.write(StaffJobRequest {
                kind: cancelled_staff_job.kind,
                target,
                urgency,
                token: Some(token),
            });
        }
    }
}

/// Whether a numeric authored cancel rule withdraws the request at `level`.
fn authored_cancel_rule_holds(row: &StaffRequestControllerDefinition, level: f32) -> bool {
    let StaffRequestThresholdValue::Number(threshold) = row.cancel_threshold else {
        return false;
    };
    match row.cancel_comparison {
        StaffRequestThresholdComparison::EntersEquality => level == threshold,
        StaffRequestThresholdComparison::FallsBelow => level < threshold,
        StaffRequestThresholdComparison::RisesAbove => level > threshold,
        StaffRequestThresholdComparison::ReachesOrBelow => level <= threshold,
        StaffRequestThresholdComparison::ReachesOrAbove => level >= threshold,
    }
}

pub(super) fn classify_spawned_staff_jobs_by_maintenance_work_kind(
    mut commands: Commands,
    spawned_staff_jobs: Query<(Entity, &StaffJob), Added<StaffJob>>,
) {
    for (staff_job_entity, staff_job) in &spawned_staff_jobs {
        match staff_job.kind {
            StaffJobKind::EmptyBin => commands
                .entity(staff_job_entity)
                .insert(WasteContainerEmptyingStaffJobKindMarker),
            StaffJobKind::SweepLitter => commands
                .entity(staff_job_entity)
                .insert(LooseLitterSweepingStaffJobKindMarker),
            _ => continue,
        };
    }
}
