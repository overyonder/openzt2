use super::*;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    BehaviorInstructionId, RunningBehaviorInstruction,
};

#[derive(Component)]
pub(super) struct PendingBehaviorObjectSpawn {
    pub(super) instruction: BehaviorInstructionId,
    pub(super) target: Option<Entity>,
    pub(super) prefab: Option<Handle<ScenePrefabAsset>>,
    pub(super) request: Option<usize>,
    pub(super) spawned: bool,
}

impl PendingBehaviorObjectSpawn {
    pub(super) fn owns(
        &self,
        task: &BehaviorTaskExecutionState,
        step: &RunningBehaviorInstruction,
    ) -> bool {
        self.instruction == step.id() && self.target == task.target && step.is_current(task)
    }
}

/// Keeps the model of an at-actor spawn loading while its actor waits for it.
#[derive(Component)]
pub(super) struct PendingAtActorSpawnPrefab {
    pub(super) object: AssetId,
    pub(super) prefab: Handle<ScenePrefabAsset>,
}
