use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::DefinitionId,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
    behavior_task_execution_types::{
        running_actions::{RunBehaviorTermination, RunningBehaviorInstruction},
        AppliedBehaviorLitterKill, BehaviorKilledTarget, BehaviorTaskExecutionState,
        BehaviorTaskFailed, BehaviorTaskFinished, PendingBehaviorTaskFailure,
    },
    maintenance::maintenance_types::LooseLitterWaste,
};

use super::{
    staff_employment_types::Staff,
    staff_job_types::{
        CurrentJob, JobClaim, StaffJob, StaffJobBehaviorTask, StaffJobCancellationPending,
    },
    staff_lifecycle_messages::StaffJobCompleted,
};

pub(super) fn acknowledge_staff_target_termination_owned_by_terminal_maintenance_systems(
    mut commands: Commands,
    clock: Res<ZooClock>,
    staff: Query<
        (
            Entity,
            &CurrentJob,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorTermination,
        ),
        (With<Staff>, Without<PendingBehaviorTaskFailure>),
    >,
    jobs: Query<(
        &StaffJob,
        &JobClaim,
        &StaffJobBehaviorTask,
        Option<&AppliedBehaviorLitterKill>,
    )>,
    litter: Query<(&DefinitionId, &GlobalTransform), With<LooseLitterWaste>>,
    assignments: Query<&CurrentJob, With<Staff>>,
    mut finished: MessageReader<BehaviorTaskFinished>,
    mut failed: MessageReader<BehaviorTaskFailed>,
    cancelling: Query<(), With<StaffJobCancellationPending>>,
    mut completed: MessageWriter<StaffJobCompleted>,
) {
    for (actor, current, task, instruction, action) in &staff {
        let Ok((job, claim, assigned, applied)) = jobs.get(current.job()) else {
            continue;
        };
        if job.kind != StaffJobKind::SweepLitter
            || !action.0.kill_target
            || action.0.kill_subject
            || task.next_action_tick > clock.tick
            || !instruction.is_current(task)
        {
            continue;
        }
        if claim.staff != actor
            || assigned.execution_id != Some(task.execution_id)
            || task.target != Some(job.target)
            || cancelling.contains(current.job())
        {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        if applied.is_some_and(|applied| applied.0 == task.execution_id) {
            instruction.complete(actor, clock.tick, &mut commands);
            continue;
        }
        let Ok((definition, transform)) = litter.get(job.target) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let job_entity = current.job();
        let killed = BehaviorKilledTarget {
            execution_id: task.execution_id,
            entity: job.target,
            definition: definition.0,
            translation: transform.translation(),
        };
        let id = instruction.id();
        let tick = clock.tick;
        commands.queue(move |world: &mut World| {
            if !world.get::<RunningBehaviorInstruction>(actor).is_some_and(|current| current.id() == id) {
                return;
            }
            if !world.despawn(killed.entity) {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut world.commands());
                return;
            }
            world.entity_mut(actor).insert(killed);
            world.entity_mut(job_entity).insert(AppliedBehaviorLitterKill(killed.execution_id));
            let mut commands = world.commands();
            openzt2_zoo_state::behavior_task_execution_types::running_actions::complete_behavior_instruction(actor, id, tick, &mut commands);
        });
    }
    for (actor, execution_id) in finished
        .read()
        .map(|event| (event.actor, event.execution_id))
        .chain(failed.read().map(|event| (event.actor, event.execution_id)))
    {
        let Ok(current) = assignments.get(actor) else {
            continue;
        };
        let Ok((job, claim, assigned, Some(applied))) = jobs.get(current.job()) else {
            continue;
        };
        if job.kind == StaffJobKind::SweepLitter
            && claim.staff == actor
            && assigned.execution_id == Some(execution_id)
            && applied.0 == execution_id
            && !cancelling.contains(current.job())
        {
            completed.write(StaffJobCompleted {
                staff: actor,
                job: current.job(),
                kind: job.kind,
                target: job.target,
            });
        }
    }
}
