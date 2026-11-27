use bevy::{ecs::entity::EntityHashSet, prelude::*};
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};
use openzt2_world_state::{
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::{
    behavior_task_execution_types::{
        running_actions::{
            behavior_action_actors, mark_behavior_task_for_failure_and_stop_navigation,
            RunBehaviorTermination, RunningBehaviorInstruction,
        },
        AppliedBehaviorLitterKill, BehaviorKilledTarget, BehaviorTaskExecutionState,
        BehaviorTaskFailed, BehaviorTaskFinished, PendingBehaviorTaskFailure,
    },
    maintenance::maintenance_types::{
        AnimalHabitatWaste, AppliedHabitatWasteCleanup, LooseLitterWaste,
    },
    staff::{
        staff_employment_types::Staff,
        staff_job_types::{
            CurrentJob, JobClaim, StaffJob, StaffJobBehaviorTask, StaffJobCancellationPending,
        },
        staff_lifecycle_messages::StaffJobCompleted,
    },
};

#[derive(Component, Default)]
pub(super) struct WasteConsumptionClaim(bool);

type WasteJob = (
    &'static StaffJob,
    &'static JobClaim,
    &'static StaffJobBehaviorTask,
    Option<&'static AppliedBehaviorLitterKill>,
    Option<&'static AppliedHabitatWasteCleanup>,
    Has<StaffJobCancellationPending>,
);

fn applied_execution(
    kind: StaffJobKind,
    litter: Option<&AppliedBehaviorLitterKill>,
    dung: Option<&AppliedHabitatWasteCleanup>,
) -> Option<u64> {
    match kind {
        StaffJobKind::SweepLitter => litter.map(|receipt| receipt.0),
        StaffJobKind::CleanHabitat => dung.map(|receipt| receipt.execution_id),
        _ => None,
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn apply_staff_waste_termination(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
    mut staff: Query<
        (
            Entity,
            &CurrentJob,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorTermination,
            Option<&mut BehaviorKilledTarget>,
        ),
        (With<Staff>, Without<PendingBehaviorTaskFailure>),
    >,
    jobs: Query<WasteJob>,
    mut targets: Query<(
        &DefinitionId,
        &GlobalTransform,
        Has<LooseLitterWaste>,
        Option<&AnimalHabitatWaste>,
        Has<WorldMember>,
        &mut WasteConsumptionClaim,
    )>,
) {
    for (actor, current, task, instruction, action, receipt) in
        behavior_action_actors(&mut staff, activation.0)
    {
        let job_entity = current.job();
        let Ok((job, claim, assigned, litter_receipt, dung_receipt, cancelling)) =
            jobs.get(job_entity)
        else {
            continue;
        };
        let Some(kind @ (StaffJobKind::SweepLitter | StaffJobKind::CleanHabitat)) = job.kind else {
            continue;
        };
        if !action.0.kill_target
            || action.0.kill_subject
            || task.next_action_tick > clock.tick
            || !instruction.is_current(task)
            || (activation.0.is_none() && !instruction.can_update_after_activation(clock.tick))
        {
            continue;
        }
        let Some(target) = job.target else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        if cancelling
            || claim.staff != actor
            || assigned.execution_id != Some(task.execution_id)
            || task.target() != Some(target)
            || (kind == StaffJobKind::CleanHabitat
                && job.token != Some(AssetId::from_key("t_rakepoo")))
        {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        if let Some(applied) = applied_execution(kind, litter_receipt, dung_receipt) {
            if applied == task.execution_id {
                instruction.complete(actor, clock.tick, &mut commands);
            } else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            }
            continue;
        }
        let Ok((definition, transform, litter, dung, in_world, mut consumed)) =
            targets.get_mut(target)
        else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let eligible = match kind {
            StaffJobKind::SweepLitter => litter,
            StaffJobKind::CleanHabitat => in_world && dung.is_some_and(|waste| waste.is_valid()),
            _ => false,
        };
        if consumed.0 || !eligible {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        consumed.0 = true;
        let killed = BehaviorKilledTarget {
            execution_id: task.execution_id,
            entity: target,
            definition: definition.0,
            translation: transform.translation(),
        };
        commands.entity(target).despawn();
        if let Some(mut receipt) = receipt {
            receipt.set_if_neq(killed);
        } else {
            commands.entity(actor).insert(killed);
        }
        if kind == StaffJobKind::SweepLitter {
            commands
                .entity(job_entity)
                .insert(AppliedBehaviorLitterKill(killed.execution_id));
        } else {
            commands
                .entity(job_entity)
                .insert(AppliedHabitatWasteCleanup {
                    execution_id: killed.execution_id,
                });
        }
        instruction.complete(actor, clock.tick, &mut commands);
    }
}

pub(super) fn settle_consumed_staff_waste_jobs(
    mut finished: MessageReader<BehaviorTaskFinished>,
    mut failed: MessageReader<BehaviorTaskFailed>,
    staff: Query<&CurrentJob, With<Staff>>,
    jobs: Query<WasteJob>,
    mut completed: MessageWriter<StaffJobCompleted>,
) {
    let mut settled = EntityHashSet::default();
    for (actor, execution_id) in finished
        .read()
        .map(|event| (event.actor, event.execution_id))
        .chain(failed.read().map(|event| (event.actor, event.execution_id)))
    {
        let Ok(current) = staff.get(actor) else {
            continue;
        };
        let job_entity = current.job();
        let Ok((job, claim, assigned, litter, dung, cancelling)) = jobs.get(job_entity) else {
            continue;
        };
        let (Some(kind), Some(target)) = (job.kind, job.target) else {
            continue;
        };
        if !cancelling
            && claim.staff == actor
            && assigned.execution_id == Some(execution_id)
            && applied_execution(kind, litter, dung) == Some(execution_id)
            && settled.insert(job_entity)
        {
            completed.write(StaffJobCompleted {
                staff: actor,
                job: job_entity,
                kind,
                target,
            });
        }
    }
}
