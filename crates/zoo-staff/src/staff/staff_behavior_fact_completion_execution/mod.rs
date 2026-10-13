use super::staff_job_completion_rules::behavior_fact_modification_is_owned_by_terminal_maintenance_system;
use bevy::prelude::*;
use openzt2_game_data::behavior::action_record::BehaviorAction;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::behavior_task_execution_types::advance_behavior_task_to_next_action;
use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;

use super::{
    staff_employment_types::Staff,
    staff_job_types::{CurrentJob, StaffJob},
};

pub(super) fn acknowledge_staff_job_completion_fact_actions_owned_by_terminal_domain_systems(
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    simulation_clock: Res<ZooClock>,
    mut staff: Query<(&CurrentJob, &mut BehaviorTaskExecutionState), With<Staff>>,
    jobs: Query<&StaffJob>,
) {
    for (current_job, mut behavior_task) in &mut staff {
        if behavior_task.next_action_tick > simulation_clock.tick {
            continue;
        }
        let Some(BehaviorAction::FactModifications(modifications)) = behavior_document_assets
            .get(&behavior_task.document)
            .and_then(|document| find_current_behavior_task_action(document, &behavior_task))
        else {
            continue;
        };
        let Ok(job) = jobs.get(current_job.job) else {
            continue;
        };
        if modifications.iter().all(|modification| {
            behavior_fact_modification_is_owned_by_terminal_maintenance_system(
                modification,
                Some(job.kind),
            )
        }) {
            advance_behavior_task_to_next_action(&mut behavior_task, simulation_clock.tick);
        }
    }
}
