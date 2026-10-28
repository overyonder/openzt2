use crate::behavior_task_execution_types::BehaviorTaskReturnFrame;
use bevy::prelude::*;

#[derive(Component)]
pub struct SynchronizedBehaviorOwner {
    pub execution_id: u64,
    pub target_execution_id: u64,
    pub target: Entity,
    pub origin: BehaviorTaskReturnFrame,
    pub depth: usize,
    pub target_outcome: Option<bool>,
}
