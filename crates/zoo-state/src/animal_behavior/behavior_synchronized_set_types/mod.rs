use crate::behavior_task_execution_types::{
    BehaviorTaskReturnFrame, SynchronizedBehaviorParticipant,
};
use bevy::prelude::*;

#[derive(Component)]
#[relationship(relationship_target = SynchronizedBehaviorOwner)]
pub struct SynchronizedBehaviorSubject(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = SynchronizedBehaviorSubject)]
pub struct SynchronizedBehaviorOwner(Entity);

#[derive(Component)]
#[relationship(relationship_target = SynchronizedBehaviorParticipant)]
pub struct SynchronizedBehaviorPartner(pub Entity);

#[derive(Component)]
pub struct SynchronizedBehaviorOperation {
    pub execution_id: u64,
    pub target_execution_id: u64,
    pub origin: BehaviorTaskReturnFrame,
    pub depth: usize,
    pub target_outcome: Option<bool>,
}
