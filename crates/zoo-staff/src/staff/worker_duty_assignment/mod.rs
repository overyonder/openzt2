use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::action::staff_management::UiWorkerDuty,
    world_definitions::staff_management::{StaffJobCapabilityFlags, StaffJobKind, StaffRoleKind},
};

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

use super::{
    staff_assignment_types::{
        CleansFilters, CleansRecycling, EmptiesTrash, SetStaffWorkerDutyAssignment, SweepsTrash,
    },
    staff_employment_types::{Staff, StaffRole},
    staff_job_eligibility::{
        staff_behavior_task_satisfies_authored_request_token, staff_subject_matches,
    },
};

/// Applies the original maintenance-worker default: an absent authored task
/// assignment list means that every maintenance duty is enabled.
///
/// The four duty components remain the authoritative live assignment facts.
/// This projection runs once for a newly identified or hired staff entity and
/// does not overwrite later player changes.
pub(super) fn enable_every_maintenance_worker_duty_for_new_staff_by_default(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    new_staff: Query<(Entity, &StaffRole), Added<Staff>>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (staff_entity, staff_role) in &new_staff {
        if definitions
            .find_staff(staff_role.0)
            .is_some_and(|definition| definition.role == StaffRoleKind::Maintenance)
        {
            commands.entity(staff_entity).insert((
                CleansFilters,
                CleansRecycling,
                EmptiesTrash,
                SweepsTrash,
            ));
        }
    }
}

pub(super) fn maintenance_worker_duties_accept_staff_job(
    staff_role_kind: StaffRoleKind,
    job_kind: StaffJobKind,
    token: Option<openzt2_game_data::AssetId>,
    cleans_filters: bool,
    cleans_recycling: bool,
    empties_trash: bool,
    sweeps_trash: bool,
) -> bool {
    if !matches!(staff_role_kind, StaffRoleKind::Maintenance) {
        return true;
    }
    match job_kind {
        StaffJobKind::MaintainTank => cleans_filters,
        StaffJobKind::EmptyBin => {
            if token == Some(worker_duty_token(UiWorkerDuty::EmptyRecyclingContainer)) {
                cleans_recycling
            } else {
                empties_trash
            }
        }
        StaffJobKind::SweepLitter => sweeps_trash,
        _ => true,
    }
}

pub(super) fn apply_staff_worker_duty_assignments(
    mut commands: Commands,
    mut assignment_requests: MessageReader<SetStaffWorkerDutyAssignment>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    loaded_behavior_documents: Res<LoadedBehaviorDocumentCollection>,
    staff_roles: Query<&StaffRole, With<Staff>>,
    capabilities: Res<openzt2_game_assets::world_definitions::loaded_staff_behavior_capabilities::LoadedStaffBehaviorCapabilities>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(behavior_declarations) =
        loaded_behavior_documents.create_declaration_index_view(&behavior_document_assets)
    else {
        return;
    };
    for request in assignment_requests.read() {
        let Ok(staff_role) = staff_roles.get(request.worker) else {
            continue;
        };
        if request.assigned
            && !staff_role_accepts_worker_duty(
                definitions,
                behavior_declarations,
                staff_role,
                request.duty,
                &capabilities,
            )
        {
            continue;
        }
        match request.duty {
            UiWorkerDuty::CleanAquaticTankFilter => {
                if request.assigned {
                    commands.entity(request.worker).insert(CleansFilters);
                } else {
                    commands.entity(request.worker).remove::<CleansFilters>();
                }
            }
            UiWorkerDuty::EmptyRecyclingContainer => {
                if request.assigned {
                    commands.entity(request.worker).insert(CleansRecycling);
                } else {
                    commands.entity(request.worker).remove::<CleansRecycling>();
                }
            }
            UiWorkerDuty::EmptyTrashContainer => {
                if request.assigned {
                    commands.entity(request.worker).insert(EmptiesTrash);
                } else {
                    commands.entity(request.worker).remove::<EmptiesTrash>();
                }
            }
            UiWorkerDuty::SweepLitter => {
                if request.assigned {
                    commands.entity(request.worker).insert(SweepsTrash);
                } else {
                    commands.entity(request.worker).remove::<SweepsTrash>();
                }
            }
        }
    }
}

fn staff_role_accepts_worker_duty(
    definitions: WorldDefinitionsView<'_>,
    behavior_declarations: openzt2_game_assets::behavior::behavior_asset_types::BehaviorDeclarationIndexView<'_>,
    staff_role: &StaffRole,
    worker_duty: UiWorkerDuty,
    capabilities: &openzt2_game_assets::world_definitions::loaded_staff_behavior_capabilities::LoadedStaffBehaviorCapabilities,
) -> bool {
    let required_job_kind = match worker_duty {
        UiWorkerDuty::CleanAquaticTankFilter => StaffJobKind::MaintainTank,
        UiWorkerDuty::EmptyRecyclingContainer | UiWorkerDuty::EmptyTrashContainer => {
            StaffJobKind::EmptyBin
        }
        UiWorkerDuty::SweepLitter => StaffJobKind::SweepLitter,
    };
    definitions
        .find_staff(staff_role.0)
        .is_some_and(|staff_role_definition| {
            let token = worker_duty_token(worker_duty);
            capabilities
                .for_employee(definitions, staff_role_definition)
                .contains_all(StaffJobCapabilityFlags::for_job_kind(required_job_kind))
                && behavior_declarations.behavior_tasks().any(|(_, _, task)| {
                    staff_behavior_task_satisfies_authored_request_token(task, Some(token))
                        && (task.subjects.is_empty()
                            || task.subjects.iter().any(|subject| {
                                staff_subject_matches(
                                    definitions,
                                    staff_role_definition,
                                    openzt2_game_data::AssetId::from_key(
                                        &subject.to_ascii_lowercase(),
                                    ),
                                )
                            }))
                })
        })
}

fn worker_duty_token(duty: UiWorkerDuty) -> openzt2_game_data::AssetId {
    openzt2_game_data::AssetId::from_key(match duty {
        UiWorkerDuty::CleanAquaticTankFilter => "t_cleanwaterfilter",
        UiWorkerDuty::EmptyRecyclingContainer => "t_emptyrecyclingbin",
        UiWorkerDuty::EmptyTrashContainer => "t_emptytrash",
        UiWorkerDuty::SweepLitter => "t_sweeptrash",
    })
}
