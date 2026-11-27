use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};

use super::{
    staff_assignment_types::{SetStaffWorkerDutyAssignment, WorkerDutyAssignments},
    staff_employment_types::{Staff, StaffRole},
    staff_job_eligibility::staff_subject_matches,
};

pub(super) fn apply_staff_worker_duty_assignments(
    mut requests: MessageReader<SetStaffWorkerDutyAssignment>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut staff: Query<(&StaffRole, &mut WorkerDutyAssignments), With<Staff>>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(manager) = catalogue.staff_manager() else {
        return;
    };
    for request in requests.read() {
        let Ok((role, mut assignments)) = staff.get_mut(request.worker) else {
            continue;
        };
        let Some(role) = catalogue.find_staff(role.0) else {
            continue;
        };
        if assignments.is_assigned(request.task) == request.assigned {
            continue;
        }
        if !manager
            .tokens
            .iter()
            .any(|rule| rule.token == request.task && rule.name.is_some())
        {
            continue;
        }
        let available = catalogue.objects().any(|object| {
            catalogue.staff_request_rows(object.id).is_some_and(|rows| {
                rows.iter().any(|row| {
                    row.request.token == Some(request.task)
                        && row
                            .request
                            .subject_type
                            .is_none_or(|subject| staff_subject_matches(catalogue, role, subject))
                })
            })
        });
        if available {
            assignments.set_assigned(request.task, request.assigned);
        }
    }
}
