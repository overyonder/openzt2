use bevy::prelude::*;
use openzt2_game_data::{
    behavior::action_record::BehaviorAction, world_definitions::staff_management::StaffJobKind,
};

use crate::{
    assets::behavior::behavior_asset_types::BehaviorDocumentAsset,
    plugins::{
        behavior_task_execution_types::{
            advance_behavior_task_to_next_action, find_current_behavior_task_action,
            BehaviorTaskExecutionState,
        },
        simulation_time::simulation_clock_types::ZooClock,
    },
};

use super::{
    staff_employment_types::Staff,
    staff_job_types::{CurrentJob, StaffJob},
};

pub(super) fn acknowledge_staff_target_termination_owned_by_terminal_maintenance_systems(
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    simulation_clock: Res<ZooClock>,
    mut staff: Query<(&CurrentJob, &mut BehaviorTaskExecutionState), With<Staff>>,
    jobs: Query<&StaffJob>,
) {
    for (current_job, mut behavior_task) in &mut staff {
        if behavior_task.next_action_tick > simulation_clock.tick {
            continue;
        }
        let Some(BehaviorAction::Termination(termination)) = behavior_document_assets
            .get(&behavior_task.document)
            .and_then(|document| find_current_behavior_task_action(document, &behavior_task))
        else {
            continue;
        };
        let Ok(job) = jobs.get(current_job.job) else {
            continue;
        };
        if termination.kill_target
            && !termination.kill_subject
            && behavior_task.target() == Some(job.target)
            && matches!(
                job.kind,
                StaffJobKind::SweepLitter | StaffJobKind::CleanHabitat
            )
        {
            advance_behavior_task_to_next_action(&mut behavior_task, simulation_clock.tick);
        }
    }
}
