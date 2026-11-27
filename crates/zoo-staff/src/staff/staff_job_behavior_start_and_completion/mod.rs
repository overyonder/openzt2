use openzt2_zoo_state::behavior_task_execution_types::allocate_behavior_execution_id;
#[cfg(test)]
mod tests;
use bevy::prelude::*;
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFailed;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFinished;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskGeneration;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::maintenance::maintenance_types::AppliedHabitatWasteCleanup;

use super::{
    staff_employment_types::Staff,
    staff_job_types::{
        CurrentJob, JobClaim, JobProgress, StaffJob, StaffJobBehaviorTask,
        StaffJobCancellationPending, StaffJobTarget, UnsuccessfulStaffJobCandidates,
    },
    staff_lifecycle_messages::StaffJobCompleted,
};

pub(super) fn start_unstarted_authored_behavior_tasks_for_claimed_staff_jobs(
    mut commands: Commands,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    mut behavior_generations: Query<&mut BehaviorTaskGeneration>,
    clocks: Query<&openzt2_world_state::simulation_time::simulation_clock_types::ZooClock>,
    staff: Query<
        (Entity, &CurrentJob, Option<&WorldMember>),
        (
            With<Staff>,
            Without<BehaviorTaskExecutionState>,
            Without<
                openzt2_zoo_state::behavior_task_execution_types::SynchronizedBehaviorParticipant,
            >,
        ),
    >,
    mut jobs: Query<(
        &StaffJob,
        Option<&StaffJobTarget>,
        &mut StaffJobBehaviorTask,
        Option<&super::staff_request_token_delivery::StaffRequestExecutionToken>,
    )>,
    tokens: Query<(
        &openzt2_zoo_state::animal_behavior::behavior_token_types::BehaviorSelfToken,
        &openzt2_zoo_state::animal_behavior::behavior_token_types::BehaviorTokenContext,
    )>,
) {
    for (staff_entity, current_job, member) in &staff {
        let Ok((job, target, mut behavior_task, dispatch)) = jobs.get_mut(current_job.job()) else {
            continue;
        };
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
            commands.entity(current_job.job()).remove::<JobClaim>();

            continue;
        };
        let tick = member
            .and_then(|member| clocks.get(member.root).ok())
            .map_or(0, |clock| clock.tick);
        let delivered_token = if let Some(dispatch) = dispatch {
            let Ok((token, context)) = tokens.get(dispatch.0) else {
                commands.entity(current_job.job()).remove::<JobClaim>();
                continue;
            };
            if !token.is_available_at(tick) {
                if tick >= token.expires_at {
                    commands.entity(current_job.job()).remove::<JobClaim>();
                }
                continue;
            }
            Some((dispatch.0, token.payload, context.force))
        } else {
            None
        };
        behavior_task.declaration_index = declaration_index;
        let execution_id = allocate_behavior_execution_id(
            staff_entity,
            behavior_generations.get_mut(staff_entity).ok(),
            &mut commands,
        );
        behavior_task.execution_id = Some(execution_id);
        if let Some((entity, _, _)) = delivered_token {
            commands.entity(entity).insert(
                openzt2_zoo_state::animal_behavior::behavior_token_types::ClaimedBehaviorSelfToken {
                    execution_id,
                }
            );
        }
        commands
            .entity(staff_entity)
            .insert(BehaviorTaskEffectiveScore {
                execution_id,
                template_score: Some(behavior_task_definition.priority.unwrap_or_default()),
                linked_score: Some(job.priority)
                    .filter(|priority| priority.is_finite() && *priority > 0.0),
                selected_candidate_score: delivered_token
                    .and_then(|(_, _, force)| force.then_some(1_000_000_000.0)),
            })
            .insert(BehaviorTaskExecutionState {
                execution_id,
                program: behavior_task_definition.id,
                target: delivered_token
                    .map_or_else(|| target.map(|target| target.0), |(_, payload, _)| payload),
                document: behavior_task.document.clone(),
                declaration: behavior_task.declaration_index,
                phase: BehaviorTaskExecutionPhase::Execution,
                action: 0,
                repetitions: 0,
                next_action_tick: 0,
                object: delivered_token.map(|(entity, _, _)| entity),
                interaction_slot: None,
                depth: 0,
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
            &mut StaffJob,
            Option<&mut UnsuccessfulStaffJobCandidates>,
        ),
        (
            Without<AppliedHabitatWasteCleanup>,
            Without<openzt2_zoo_state::behavior_task_execution_types::AppliedBehaviorLitterKill>,
        ),
    >,
) {
    for failed_behavior_task in failed_behavior_tasks.read() {
        let Ok(current_job) = staff.get(failed_behavior_task.actor) else {
            continue;
        };
        let Ok((claim, behavior_task, mut job, unsuccessful)) = jobs.get_mut(current_job.job())
        else {
            continue;
        };
        if claim.staff != failed_behavior_task.actor
            || behavior_task.execution_id != Some(failed_behavior_task.execution_id)
        {
            continue;
        }
        job.reborrow()
            .map_unchanged(|job| &mut job.first_attempt)
            .set_if_neq(false);
        if let Some(mut unsuccessful) = unsuccessful {
            unsuccessful.record(claim.staff);
        } else {
            let mut unsuccessful = UnsuccessfulStaffJobCandidates::default();
            unsuccessful.record(claim.staff);
            commands.entity(current_job.job()).insert(unsuccessful);
        }
        commands.entity(current_job.job()).remove::<JobClaim>();
    }
}

pub(crate) fn settle_staff_jobs_after_authored_behavior_finishes(
    mut commands: Commands,
    mut finished_behavior_tasks: MessageReader<BehaviorTaskFinished>,
    staff: Query<&CurrentJob, With<Staff>>,
    jobs: Query<(&StaffJob, &JobClaim, &StaffJobBehaviorTask), Without<AppliedHabitatWasteCleanup>>,
    mut completed_jobs: MessageWriter<StaffJobCompleted>,
    live_animals: Query<(), (With<Animal>, With<WorldMember>, Without<Dead>)>,
    cancelling: Query<(), With<StaffJobCancellationPending>>,
) {
    for finished_behavior_task in finished_behavior_tasks.read() {
        let Ok(current_job) = staff.get(finished_behavior_task.actor) else {
            continue;
        };
        let Ok((job, claim, behavior_task)) = jobs.get(current_job.job()) else {
            continue;
        };
        if claim.staff != finished_behavior_task.actor
            || behavior_task.execution_id != Some(finished_behavior_task.execution_id)
        {
            continue;
        }
        let Some(kind) = job.kind else {
            // The controller owns the request. The authored task owns its effects.
            commands.entity(current_job.job()).remove::<JobClaim>();
            continue;
        };
        if matches!(
            kind,
            StaffJobKind::MaintainTank | StaffJobKind::EmptyBin | StaffJobKind::SweepLitter
        ) {
            continue;
        }
        if (kind == StaffJobKind::Treat
            && (job.token == Some(AssetId::from_key("t_cureanimal"))
                || job.token == Some(AssetId::from_key("t_healanimal"))))
            || (kind == StaffJobKind::Feed && job.token == Some(AssetId::from_key("t_feedanimal")))
            || (kind == StaffJobKind::RefillWater
                && job.token == Some(AssetId::from_key("t_wateranimal")))
            || (kind == StaffJobKind::Capture
                && job.token == Some(AssetId::from_key("t_crateanimal")))
        {
            let Some(target) = job.target.filter(|&target| live_animals.contains(target)) else {
                continue;
            };
            if cancelling.contains(current_job.job()) {
                continue;
            }
            // The authored cure, heal, food placement or dart already applied
            // its effect.
            // Do not start a second, definition-driven effect after completion;
            // a new request needs a fresh threshold crossing.
            completed_jobs.write(StaffJobCompleted {
                staff: claim.staff,
                job: current_job.job(),
                kind,
                target,
            });
        } else if kind == StaffJobKind::CleanHabitat
            || (kind == StaffJobKind::Feed
                && job.token == Some(AssetId::from_key("t_fillfoodcontainer")))
        {
            // Native care completion clears the assignment; ordinary container
            // requests remain owned by their quantity threshold controller. The
            // authored task already applied the refill, so there is no second
            // quantity write or terminal feeding transaction here. A cleanup
            // task without an applied kill likewise cannot manufacture an effect.
            commands.entity(current_job.job()).remove::<JobClaim>();
        } else {
            commands.entity(current_job.job()).insert(JobProgress);
        }
    }
}
