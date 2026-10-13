//! In-flight account settlement and named-chain position for a behavior action.

use crate::behavior_task_execution_types::BehaviorTaskExecutionState;
use crate::behavior_task_execution_types::BehaviorTaskReturnFrame;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component)]
pub struct PendingBehaviorTransaction {
    pub execution_id: u64,
    pub stack_depth: usize,
    pub origin: BehaviorTaskReturnFrame,
    pub facility: Entity,
    pub transaction: AssetId,
    pub cost_override: Option<f32>,
    pub payment: Option<Entity>,
    pub next_transaction: Option<AssetId>,
}

impl PendingBehaviorTransaction {
    pub fn owns(&self, task: &BehaviorTaskExecutionState) -> bool {
        self.execution_id == task.execution_id
            && task.target == Some(self.facility)
            && task.stack.len() == self.stack_depth
            && task.is_at_or_nested_under(&self.origin, self.stack_depth)
    }
}

#[derive(Component)]
pub struct BehaviorTransactionPayment {
    pub actor: Entity,
    pub facility: Entity,
    pub transaction: AssetId,
}

/// The following quantity instruction commits with this payment, preventing
/// interruption from leaving a paid, empty container.
#[derive(Component)]
pub struct KeeperRefillPayment {
    pub job: Entity,
    pub definition: AssetId,
}

/// Usage belongs to the account's live entity; purchase and departure update it.
#[derive(Component, Default)]
pub struct EconomyUsage {
    active_users: i32,
    total_users: i32,
}

impl EconomyUsage {
    pub fn apply_user_change(&mut self, amount: i32, adding: bool) {
        if adding {
            self.active_users = self.active_users.saturating_add(amount);
            self.total_users = self.total_users.saturating_add(amount);
        } else {
            self.active_users = self.active_users.saturating_sub(amount);
        }
    }
}
