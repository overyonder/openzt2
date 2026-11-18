use super::*;

#[derive(Component)]
pub(super) struct PendingBehaviorObjectSpawn {
    pub(super) execution_id: u64,
    pub(super) origin: BehaviorTaskReturnFrame,
    pub(super) depth: usize,
    pub(super) target: Option<Entity>,
    pub(super) prefab: Option<Handle<ScenePrefabAsset>>,
    pub(super) request: Option<usize>,
    pub(super) spawned: bool,
}

impl PendingBehaviorObjectSpawn {
    pub(super) fn owns(&self, task: &BehaviorTaskExecutionState) -> bool {
        self.execution_id == task.execution_id
            && self.target == task.target
            && self.depth == task.stack.len()
            && task.is_at_or_nested_under(&self.origin, self.depth)
    }
}

/// Keeps the model of an at-actor spawn loading while its actor waits for it.
#[derive(Component)]
pub(super) struct PendingAtActorSpawnPrefab {
    pub(super) object: AssetId,
    pub(super) prefab: Handle<ScenePrefabAsset>,
}
