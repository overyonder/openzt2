//! Transition of one behavior task into failure with navigation cancellation.

use bevy::prelude::*;

use crate::locomotion::locomotion_types::Destination;
use crate::locomotion::locomotion_types::Docking;

use crate::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use crate::behavior_task_execution_types::PendingBehaviorDockingCompletion;
use crate::behavior_task_execution_types::PendingBehaviorTaskFailure;

/// Reports the calling failure site at debug level, so a task that keeps
/// failing names the check that refused it.
#[track_caller]
pub fn mark_behavior_task_for_failure_and_stop_navigation(
    actor_entity: Entity,
    commands: &mut Commands,
) {
    debug!(actor = ?actor_entity, site = %std::panic::Location::caller(),
        "behavior task marked for failure");
    commands
        .entity(actor_entity)
        .insert(PendingBehaviorTaskFailure)
        .remove::<(
            Destination,
            Docking,
            PendingBehaviorDockingCompletion,
            crate::behavior_task_execution_types::PendingBehaviorMoveCompletion,
            PendingBehaviorAnimationClipCompletion,
        )>();
}
