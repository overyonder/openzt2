use arrayvec::ArrayVec;
use bevy::prelude::*;
use openzt2_game_data::{
    behavior::action_record::BehaviorAction, world_definitions::staff_management::StaffRoleKind,
    AssetId,
};

use openzt2_game_assets::behavior::behavior_asset_types::{
    BehaviorDocumentAsset, LoadedBehaviorDocumentCollection,
};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::behavior_task_execution_types::{
    advance_behavior_task_to_next_action, find_current_behavior_task_action,
    BehaviorTaskExecutionPhase, BehaviorTaskExecutionSequence, BehaviorTaskExecutionState,
    PendingBehaviorTaskFailure, SynchronizedBehaviorParticipant,
};
use openzt2_zoo_state::locomotion::locomotion_types::{Destination, NavAgent};

use super::{
    staff_employment_types::{AvailableForWork, Staff, StaffRole},
    staff_job_types::CurrentJob,
};

/// Staff with no job run the shared `staff:StaffWander` task from
/// `Staff.tsk`, as the original does when no work scores higher.
/// ponytail: wander only; `StaffWaitBench` and `DefaultTask` are not scored.
pub(crate) fn start_idle_staff_wander_tasks(
    mut commands: Commands,
    documents: Res<LoadedBehaviorDocumentCollection>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut sequence: ResMut<BehaviorTaskExecutionSequence>,
    idle: Query<
        (Entity, &StaffRole),
        (
            With<Staff>,
            With<AvailableForWork>,
            With<NavAgent>,
            Without<CurrentJob>,
            Without<BehaviorTaskExecutionState>,
            Without<Destination>,
            Without<SynchronizedBehaviorParticipant>,
        ),
    >,
) {
    if idle.is_empty() {
        return;
    }
    let (Some(view), Some(catalogue)) = (
        documents.create_declaration_index_view(&assets),
        active_definitions.get(&definitions),
    ) else {
        return;
    };
    let Some((document, declaration, task)) =
        view.find_behavior_task_location(AssetId::from_key("staff:staffwander"))
    else {
        return;
    };
    for (staff, role) in &idle {
        // Recovery teams follow each other through their own idle tasks.
        if catalogue
            .find_staff(role.0)
            .is_none_or(|role| role.role == StaffRoleKind::Recovery)
        {
            continue;
        }
        commands.entity(staff).insert(BehaviorTaskExecutionState {
            execution_id: sequence.next(),
            program: task.id,
            target: Some(staff),
            document: document.clone(),
            declaration,
            phase: BehaviorTaskExecutionPhase::Execution,
            action: 0,
            repetitions: 0,
            next_action_tick: 0,
            object: None,
            interaction_slot: None,
            stack: ArrayVec::new(),
        });
    }
}

/// ponytail: staff never stand inside a tank yet, so leaving one is
/// immediate; the gate route is needed once staff enter tanks.
pub(crate) fn exit_tank_gates_immediately(
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    mut actors: Query<&mut BehaviorTaskExecutionState, Without<PendingBehaviorTaskFailure>>,
) {
    for mut task in &mut actors {
        if task.next_action_tick <= clock.tick
            && matches!(
                documents
                    .get(&task.document)
                    .and_then(|document| find_current_behavior_task_action(document, &task)),
                Some(BehaviorAction::ExitTankGate)
            )
        {
            advance_behavior_task_to_next_action(&mut task, clock.tick);
        }
    }
}
