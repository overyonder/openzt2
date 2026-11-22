use bevy::prelude::*;
use openzt2_game_assets::behavior::behavior_asset_types::{
    BehaviorDeclarationIndexView, BehaviorDocumentAsset, LoadedBehaviorDocumentCollection,
};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::{
    WorldDefinitions, WorldDefinitionsView,
};
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::{
    ui_document::action::staff_management::UiWorkerDuty,
    world_definitions::staff_management::{StaffJobKind, StaffRoleDefinition, StaffRoleKind},
    AssetId,
};

use super::{
    staff_assignment_types::{
        CleansFilters, CleansRecycling, EmptiesTrash, SetStaffWorkerDutyAssignment, SweepsTrash,
    },
    staff_employment_types::{Staff, StaffJobCapabilities, StaffRole},
    staff_job_eligibility::{staff_subject_matches, StaffTasksByRequestToken},
};

pub(super) type WorkerDutyFlags = (
    Has<CleansFilters>,
    Has<CleansRecycling>,
    Has<EmptiesTrash>,
    Has<SweepsTrash>,
);

const WORKER_DUTIES: [UiWorkerDuty; 4] = [
    UiWorkerDuty::CleanAquaticTankFilter,
    UiWorkerDuty::EmptyRecyclingContainer,
    UiWorkerDuty::EmptyTrashContainer,
    UiWorkerDuty::SweepLitter,
];

pub(super) fn initialize_supported_worker_duties(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    behavior_assets: Res<Assets<BehaviorDocumentAsset>>,
    behavior_documents: Res<LoadedBehaviorDocumentCollection>,
    token_tasks: Res<StaffTasksByRequestToken>,
    staff: Query<
        (Entity, &StaffRole, &StaffJobCapabilities),
        (With<Staff>, Added<StaffJobCapabilities>),
    >,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(declarations) = behavior_documents.create_declaration_index_view(&behavior_assets)
    else {
        return;
    };
    for (employee, role, capabilities) in &staff {
        let Some(role) = catalogue
            .find_staff(role.0)
            .filter(|role| role.role == StaffRoleKind::Maintenance)
        else {
            continue;
        };
        let mut employee = commands.entity(employee);
        for duty in WORKER_DUTIES {
            if staff_supports_worker_duty(
                catalogue,
                declarations,
                &token_tasks,
                role,
                *capabilities,
                duty,
            ) {
                set_worker_duty(&mut employee, duty, true);
            }
        }
    }
}

pub(super) fn apply_staff_worker_duty_assignments(
    mut commands: Commands,
    mut assignment_requests: MessageReader<SetStaffWorkerDutyAssignment>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    behavior_assets: Res<Assets<BehaviorDocumentAsset>>,
    behavior_documents: Res<LoadedBehaviorDocumentCollection>,
    token_tasks: Res<StaffTasksByRequestToken>,
    staff: Query<(&StaffRole, Option<&StaffJobCapabilities>), With<Staff>>,
) {
    let catalogue = active_definitions.get(&definitions);
    let declarations = behavior_documents.create_declaration_index_view(&behavior_assets);
    for request in assignment_requests.read() {
        let Ok((role, capabilities)) = staff.get(request.worker) else {
            continue;
        };
        if request.assigned {
            let (Some(catalogue), Some(declarations), Some(capabilities)) =
                (catalogue, declarations, capabilities)
            else {
                continue;
            };
            if !catalogue.find_staff(role.0).is_some_and(|role| {
                staff_supports_worker_duty(
                    catalogue,
                    declarations,
                    &token_tasks,
                    role,
                    *capabilities,
                    request.duty,
                )
            }) {
                continue;
            }
        }
        set_worker_duty(
            &mut commands.entity(request.worker),
            request.duty,
            request.assigned,
        );
    }
}

fn set_worker_duty(employee: &mut EntityCommands, duty: UiWorkerDuty, assigned: bool) {
    match (duty, assigned) {
        (UiWorkerDuty::CleanAquaticTankFilter, true) => {
            employee.insert_if_new(CleansFilters);
        }
        (UiWorkerDuty::CleanAquaticTankFilter, false) => {
            employee.remove::<CleansFilters>();
        }
        (UiWorkerDuty::EmptyRecyclingContainer, true) => {
            employee.insert_if_new(CleansRecycling);
        }
        (UiWorkerDuty::EmptyRecyclingContainer, false) => {
            employee.remove::<CleansRecycling>();
        }
        (UiWorkerDuty::EmptyTrashContainer, true) => {
            employee.insert_if_new(EmptiesTrash);
        }
        (UiWorkerDuty::EmptyTrashContainer, false) => {
            employee.remove::<EmptiesTrash>();
        }
        (UiWorkerDuty::SweepLitter, true) => {
            employee.insert_if_new(SweepsTrash);
        }
        (UiWorkerDuty::SweepLitter, false) => {
            employee.remove::<SweepsTrash>();
        }
    }
}

fn staff_supports_worker_duty(
    catalogue: WorldDefinitionsView<'_>,
    declarations: BehaviorDeclarationIndexView<'_>,
    token_tasks: &StaffTasksByRequestToken,
    role: &StaffRoleDefinition,
    capabilities: StaffJobCapabilities,
    duty: UiWorkerDuty,
) -> bool {
    let kind = match duty {
        UiWorkerDuty::CleanAquaticTankFilter => StaffJobKind::MaintainTank,
        UiWorkerDuty::EmptyRecyclingContainer | UiWorkerDuty::EmptyTrashContainer => {
            StaffJobKind::EmptyBin
        }
        UiWorkerDuty::SweepLitter => StaffJobKind::SweepLitter,
    };
    capabilities.supports(kind)
        && token_tasks.candidates(worker_duty_token(duty)).any(|task| {
            declarations
                .find_behavior_task_location(task)
                .is_some_and(|(_, _, task)| {
                    task.subjects.is_empty()
                        || task.subjects.iter().any(|subject| {
                            staff_subject_matches(
                                catalogue,
                                role,
                                AssetId::from_key(&subject.trim().to_ascii_lowercase()),
                            )
                        })
                })
        })
}

pub(super) fn worker_duties_allow_staff_job(
    role: StaffRoleKind,
    kind: StaffJobKind,
    token: AssetId,
    capabilities: StaffJobCapabilities,
    (filters, recycling, trash, sweeping): (bool, bool, bool, bool),
) -> bool {
    if role != StaffRoleKind::Maintenance {
        return true;
    }
    match kind {
        StaffJobKind::MaintainTank
            if token == worker_duty_token(UiWorkerDuty::CleanAquaticTankFilter) =>
        {
            capabilities.supports(kind) && filters
        }
        StaffJobKind::EmptyBin
            if token == worker_duty_token(UiWorkerDuty::EmptyRecyclingContainer) =>
        {
            capabilities.supports(kind) && recycling
        }
        StaffJobKind::EmptyBin if token == worker_duty_token(UiWorkerDuty::EmptyTrashContainer) => {
            capabilities.supports(kind) && trash
        }
        StaffJobKind::SweepLitter if token == worker_duty_token(UiWorkerDuty::SweepLitter) => {
            capabilities.supports(kind) && sweeping
        }
        _ => true,
    }
}

fn worker_duty_token(duty: UiWorkerDuty) -> AssetId {
    AssetId::from_key(match duty {
        UiWorkerDuty::CleanAquaticTankFilter => "t_cleanwaterfilter",
        UiWorkerDuty::EmptyRecyclingContainer => "t_emptyrecyclingbin",
        UiWorkerDuty::EmptyTrashContainer => "t_emptytrash",
        UiWorkerDuty::SweepLitter => "t_sweeptrash",
    })
}
