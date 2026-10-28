use arrayvec::ArrayVec;

#[cfg(test)]
mod tests;
use bevy::{platform::collections::HashSet, prelude::*};
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionSequence;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFailed;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFinished;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::maintenance::maintenance_types::AppliedHabitatWasteCleanup;

use super::{
    staff_employment_types::{AvailableForWork, Staff},
    staff_job_types::{
        CurrentJob, JobClaim, JobProgress, StaffJob, StaffJobBehaviorTask,
        UnsuccessfulStaffJobCandidates,
    },
    staff_lifecycle_messages::{CancelStaffJobRequest, StaffJobCompleted},
};

pub(crate) fn start_unstarted_authored_behavior_tasks_for_claimed_staff_jobs(
    mut commands: Commands,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    mut execution_sequence: ResMut<BehaviorTaskExecutionSequence>,
    staff: Query<
        (Entity, &CurrentJob),
        (
            With<Staff>,
            Without<BehaviorTaskExecutionState>,
            Without<
                openzt2_zoo_state::behavior_task_execution_types::SynchronizedBehaviorParticipant,
            >,
        ),
    >,
    mut jobs: Query<(&StaffJob, &mut StaffJobBehaviorTask, &JobClaim)>,
) {
    for (staff_entity, current_job) in &staff {
        let Ok((job, mut behavior_task, claim)) = jobs.get_mut(current_job.job) else {
            commands
                .entity(staff_entity)
                .remove::<CurrentJob>()
                .insert(AvailableForWork);
            continue;
        };
        if claim.staff != staff_entity {
            commands
                .entity(staff_entity)
                .remove::<CurrentJob>()
                .insert(AvailableForWork);
            continue;
        }
        if behavior_task.execution_id.is_some() {
            continue;
        }
        let Some(document) = behavior_document_assets.get(&behavior_task.document) else {
            // Async asset availability is not cancellation of the care request.
            // Retry this unstarted assignment when its canonical document is ready.
            continue;
        };
        // The live asset may have reordered its declarations since selection.
        // Match its canonical declaration identity in that document, retaining
        // the same last-declaration precedence as the behavior asset index.
        let selected_task = (0..)
            .map_while(|index| {
                document
                    .behavior_task_at_index(index)
                    .map(|task| (index, task))
            })
            .filter(|(_, task)| task.id == behavior_task.task)
            .last();
        let Some((declaration_index, behavior_task_definition)) = selected_task else {
            // Removed source data invalidates this assignment, not the care
            // request. Its controller remains the owner of request lifetime.
            commands
                .entity(current_job.job)
                .remove::<(JobClaim, StaffJobBehaviorTask, JobProgress)>();
            commands
                .entity(staff_entity)
                .remove::<CurrentJob>()
                .insert(AvailableForWork);
            continue;
        };
        behavior_task.declaration_index = declaration_index;
        let execution_id = execution_sequence.next();
        behavior_task.execution_id = Some(execution_id);
        commands
            .entity(staff_entity)
            .insert(BehaviorTaskEffectiveScore {
                execution_id,
                template_score: Some(behavior_task_definition.priority.unwrap_or_default()),
                linked_score: Some(f32::from_bits(u32::MAX - job.urgency))
                    .filter(|priority| priority.is_finite() && *priority > 0.0),
                selected_candidate_score: None,
            })
            .insert(BehaviorTaskExecutionState {
                execution_id,
                program: behavior_task_definition.id,
                target: Some(job.target),
                document: behavior_task.document.clone(),
                declaration: behavior_task.declaration_index,
                phase: BehaviorTaskExecutionPhase::Execution,
                action: 0,
                repetitions: 0,
                next_action_tick: 0,
                object: None,
                interaction_slot: None,
                stack: ArrayVec::new(),
            })
            .remove::<Destination>();
    }
}

pub(super) fn release_staff_assignment_after_authored_behavior_failure_finishes(
    mut commands: Commands,
    mut failed_behavior_tasks: MessageReader<BehaviorTaskFailed>,
    staff: Query<&CurrentJob, With<Staff>>,
    mut jobs: Query<
        (
            &JobClaim,
            &StaffJobBehaviorTask,
            Option<&mut UnsuccessfulStaffJobCandidates>,
        ),
        Without<AppliedHabitatWasteCleanup>,
    >,
) {
    for failed_behavior_task in failed_behavior_tasks.read() {
        let Ok(current_job) = staff.get(failed_behavior_task.actor) else {
            continue;
        };
        let Ok((claim, behavior_task, unsuccessful)) = jobs.get_mut(current_job.job) else {
            continue;
        };
        if claim.staff != failed_behavior_task.actor
            || behavior_task.execution_id != Some(failed_behavior_task.execution_id)
        {
            continue;
        }
        if let Some(mut unsuccessful) = unsuccessful {
            unsuccessful.record(claim.staff);
        } else {
            let mut unsuccessful = UnsuccessfulStaffJobCandidates::default();
            unsuccessful.record(claim.staff);
            commands.entity(current_job.job).insert(unsuccessful);
        }
        commands
            .entity(current_job.job)
            .remove::<(JobClaim, StaffJobBehaviorTask, JobProgress)>();
        commands
            .entity(claim.staff)
            .remove::<CurrentJob>()
            .insert(AvailableForWork);
    }
}

pub(crate) fn settle_staff_jobs_after_authored_behavior_finishes(
    mut commands: Commands,
    mut finished_behavior_tasks: MessageReader<BehaviorTaskFinished>,
    staff: Query<&CurrentJob, With<Staff>>,
    jobs: Query<(&StaffJob, &JobClaim, &StaffJobBehaviorTask), Without<AppliedHabitatWasteCleanup>>,
    mut completed_jobs: MessageWriter<StaffJobCompleted>,
    live_animals: Query<(), (With<Animal>, With<WorldMember>, Without<Dead>)>,
    mut cancellations: MessageReader<CancelStaffJobRequest>,
    mut cancelled_jobs: Local<HashSet<Entity>>,
) {
    cancelled_jobs.clear();
    cancelled_jobs.extend(cancellations.read().map(|request| request.job));
    for finished_behavior_task in finished_behavior_tasks.read() {
        let Ok(current_job) = staff.get(finished_behavior_task.actor) else {
            continue;
        };
        let Ok((job, claim, behavior_task)) = jobs.get(current_job.job) else {
            continue;
        };
        if claim.staff != finished_behavior_task.actor
            || behavior_task.execution_id != Some(finished_behavior_task.execution_id)
        {
            continue;
        }
        if (job.kind == StaffJobKind::Treat
            && (job.token == Some(AssetId::from_key("t_cureanimal"))
                || job.token == Some(AssetId::from_key("t_healanimal"))))
            || (job.kind == StaffJobKind::Feed
                && job.token == Some(AssetId::from_key("t_feedanimal")))
            || (job.kind == StaffJobKind::RefillWater
                && job.token == Some(AssetId::from_key("t_wateranimal")))
            || (job.kind == StaffJobKind::Capture
                && job.token == Some(AssetId::from_key("t_crateanimal")))
        {
            if cancelled_jobs.contains(&current_job.job) || !live_animals.contains(job.target) {
                continue;
            }
            // The authored cure, heal, food placement or dart already applied
            // its effect.
            // Do not start a second, definition-driven effect after completion;
            // a new request needs a fresh threshold crossing.
            completed_jobs.write(StaffJobCompleted {
                staff: claim.staff,
                job: current_job.job,
                kind: job.kind,
                target: job.target,
            });
        } else if job.kind == StaffJobKind::CleanHabitat
            || (job.kind == StaffJobKind::Feed
                && job.token == Some(AssetId::from_key("t_fillfoodcontainer")))
        {
            // Native care completion clears the assignment; ordinary container
            // requests remain owned by their quantity threshold controller. The
            // authored task already applied the refill, so there is no second
            // quantity write or terminal feeding transaction here. A cleanup
            // task without an applied kill likewise cannot manufacture an effect.
            commands
                .entity(current_job.job)
                .remove::<(JobClaim, StaffJobBehaviorTask, JobProgress)>();
            commands
                .entity(claim.staff)
                .remove::<CurrentJob>()
                .insert(AvailableForWork);
        } else {
            commands.entity(current_job.job).insert(JobProgress);
        }
    }
}
