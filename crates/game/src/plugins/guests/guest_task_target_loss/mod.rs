//! A guest's task fails when the object it targets is gone.

use bevy::prelude::*;

use crate::plugins::animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use crate::plugins::behavior_task_execution_types::{
    BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
};

use super::guest_simulation_types::Guest;

/// A stand sold or undone under a guest walking to it, queueing at it or
/// buying from it ends that task through its authored failure branch, so the
/// guest neither pays nor waits on a target that no longer exists and is free
/// to choose another task.
pub(super) fn fail_guest_tasks_whose_target_is_gone(
    guests: Query<
        (Entity, &BehaviorTaskExecutionState),
        (With<Guest>, Without<PendingBehaviorTaskFailure>),
    >,
    entities: Query<()>,
    mut commands: Commands,
) {
    for (guest, task) in &guests {
        let Some(target) = task.target() else {
            continue;
        };
        if target != guest && !entities.contains(target) {
            debug!(
                ?guest,
                ?target,
                "guest task target is gone; failing the task"
            );
            mark_behavior_task_for_failure_and_stop_navigation(guest, &mut commands);
        }
    }
}
