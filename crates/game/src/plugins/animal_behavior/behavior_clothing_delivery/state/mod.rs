use crate::plugins::behavior_task_execution_types::{
    BehaviorTaskExecutionState, BehaviorTaskReturnFrame,
};
use bevy::prelude::*;

pub(in crate::plugins::animal_behavior) struct ClothingScriptTexture {
    pub(in crate::plugins::animal_behavior) material: String,
    pub(in crate::plugins::animal_behavior) image: String,
}

#[derive(Component)]
pub(in crate::plugins::animal_behavior) struct PendingClothingDelivery {
    pub(super) execution_id: u64,
    pub(super) origin: BehaviorTaskReturnFrame,
    pub(super) stack_depth: usize,
    pub(super) target: Option<Entity>,
    pub(super) textures: Vec<(String, Handle<Image>)>,
}

impl PendingClothingDelivery {
    pub(super) fn owns(&self, task: &BehaviorTaskExecutionState) -> bool {
        self.execution_id == task.execution_id
            && self.target == task.target()
            && self.stack_depth == task.stack.len()
            && task.is_at_or_nested_under(&self.origin, self.stack_depth)
    }
}
