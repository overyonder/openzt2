use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorCureDisease, RunningBehaviorInstruction,
};

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::staff::staff_employment_types::Staff;
use openzt2_zoo_state::staff::staff_job_types::CurrentJob;
use openzt2_zoo_state::staff::staff_job_types::JobClaim;
use openzt2_zoo_state::staff::staff_job_types::StaffJob;
use openzt2_zoo_state::staff::staff_job_types::StaffJobBehaviorTask;
use openzt2_zoo_state::staff::staff_job_types::StaffJobCancellationPending;

use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_health::types::Disease;
use openzt2_zoo_state::animal_health::types::Treatment;
use openzt2_zoo_state::animal_health::types::Vitality;

/// The authored cure action clears disease at its execution point, after the
/// approach and animation. It does not prescribe a vitality or need adjustment.
pub(crate) fn apply_authored_disease_cures_for_live_staff_claims(
    mut commands: Commands,
    clock: Res<ZooClock>,
    staff: Query<
        (
            Entity,
            Option<&CurrentJob>,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorCureDisease,
        ),
        (With<Staff>, Without<PendingBehaviorTaskFailure>),
    >,
    jobs: Query<
        (&StaffJob, &JobClaim, &StaffJobBehaviorTask),
        Without<StaffJobCancellationPending>,
    >,
    animals: Query<&Vitality, (With<Animal>, With<WorldMember>, Without<Dead>)>,
) {
    for (actor, current, task, instruction, _) in &staff {
        if task.next_action_tick > clock.tick || !instruction.is_current(task) {
            continue;
        }
        let target = current
            .and_then(|current| jobs.get(current.job()).ok())
            .filter(|(job, claim, behavior)| {
                job.kind == Some(StaffJobKind::Treat)
                    && job.token == Some(AssetId::from_key("t_cureanimal"))
                    && claim.staff == actor
                    && behavior.execution_id == Some(task.execution_id)
                    && task.target == Some(job.target)
                    && animals
                        .get(job.target)
                        .is_ok_and(|vitality| vitality.0 > 0.0)
            })
            .map(|(job, _, _)| job.target);
        let Some(target) = target else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        commands.entity(target).remove::<(Disease, Treatment)>();
        instruction.complete(actor, clock.tick, &mut commands);
    }
}
