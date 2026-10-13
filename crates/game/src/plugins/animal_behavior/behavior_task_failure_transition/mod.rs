//! Transition of one behavior task into failure with navigation cancellation.

use bevy::prelude::*;

use crate::plugins::locomotion::locomotion_types::{Destination, Docking};

use crate::plugins::behavior_task_execution_types::{
    PendingBehaviorAnimationClipCompletion, PendingBehaviorDockingCompletion,
    PendingBehaviorTaskFailure,
};

/// Reports the calling failure site at debug level, so a task that keeps
/// failing names the check that refused it.
#[track_caller]
pub(crate) fn mark_behavior_task_for_failure_and_stop_navigation(
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
            crate::plugins::animal_behavior::behavior_move_execution::PendingBehaviorMoveCompletion,
            PendingBehaviorAnimationClipCompletion,
        )>();
}
