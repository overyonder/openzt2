use bevy::prelude::*;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    BehaviorInstructionId, RunningBehaviorInstruction,
};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;

pub(crate) struct ClothingScriptTexture {
    pub(crate) material: String,
    pub(crate) image: String,
}

#[derive(Component)]
pub(crate) struct PendingClothingDelivery {
    pub(super) instruction: BehaviorInstructionId,
    pub(super) target: Option<Entity>,
    pub(super) textures: Vec<(String, Handle<Image>)>,
}

impl PendingClothingDelivery {
    pub(super) fn owns(
        &self,
        task: &BehaviorTaskExecutionState,
        instruction: &RunningBehaviorInstruction,
    ) -> bool {
        self.instruction == instruction.id()
            && self.target == task.target()
            && instruction.is_current(task)
    }
}
