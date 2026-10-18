use bevy::prelude::*;
use openzt2_game_assets::{
    application_lifecycle::GamePhase, behavior::behavior_asset_types::BehaviorDocumentAsset,
};
use openzt2_game_data::behavior::action_record::BehaviorAction;
use openzt2_world_state::{
    application_schedule::FixedGameSet, simulation_time::simulation_clock_types::ZooClock,
};
use openzt2_zoo_state::behavior_task_execution_types::{
    advance_behavior_task_to_next_action, find_current_behavior_task_action,
    BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
};

pub(super) fn register_face_target_execution(application: &mut App) {
    application.add_systems(
        FixedUpdate,
        face_current_behavior_targets
            .after(super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
            .in_set(FixedGameSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    );
}

/// ponytail: the turn is immediate; a timed turn clip is the upgrade if the
/// snap shows.
fn face_current_behavior_targets(
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    mut actors: Query<
        (&mut BehaviorTaskExecutionState, &mut Transform),
        Without<PendingBehaviorTaskFailure>,
    >,
    targets: Query<&GlobalTransform>,
) {
    for (mut task, mut transform) in &mut actors {
        if task.next_action_tick > clock.tick
            || !matches!(
                documents
                    .get(&task.document)
                    .and_then(|document| find_current_behavior_task_action(document, &task)),
                Some(BehaviorAction::FaceTarget)
            )
        {
            continue;
        }
        if let Some(target) = task
            .target()
            .and_then(|target| targets.get(target).ok())
            .map(GlobalTransform::translation)
        {
            let toward = (target - transform.translation).xz();
            if let Some(direction) = toward.try_normalize() {
                // Actor models face local +Z.
                transform.rotation = Quat::from_rotation_y(direction.x.atan2(direction.y));
            }
        }
        advance_behavior_task_to_next_action(&mut task, clock.tick);
    }
}
