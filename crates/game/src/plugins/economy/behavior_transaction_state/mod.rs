//! In-flight account settlement and named-chain position for a behavior action.

use crate::plugins::behavior_task_execution_types::{
    BehaviorTaskExecutionState, BehaviorTaskReturnFrame,
};
use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component)]
pub(super) struct PendingBehaviorTransaction {
    pub(super) execution_id: u64,
    pub(super) stack_depth: usize,
    pub(super) origin: BehaviorTaskReturnFrame,
    pub(super) facility: Entity,
    pub(super) transaction: AssetId,
    pub(super) cost_override: Option<f32>,
    pub(super) payment: Option<Entity>,
    pub(super) next_transaction: Option<AssetId>,
}

impl PendingBehaviorTransaction {
    pub(super) fn owns(&self, task: &BehaviorTaskExecutionState) -> bool {
        self.execution_id == task.execution_id
            && task.target == Some(self.facility)
            && task.stack.len() == self.stack_depth
            && task.is_at_or_nested_under(&self.origin, self.stack_depth)
    }
}

#[derive(Component)]
pub(super) struct BehaviorTransactionPayment {
    pub(super) actor: Entity,
    pub(super) facility: Entity,
    pub(super) transaction: AssetId,
}

/// Usage belongs to the account's live entity; purchase and departure update it.
#[derive(Component, Default)]
pub(crate) struct EconomyUsage {
    active_users: i32,
    total_users: i32,
}

impl EconomyUsage {
    pub(super) fn apply_user_change(&mut self, amount: i32, adding: bool) {
        if adding {
            self.active_users = self.active_users.saturating_add(amount);
            self.total_users = self.total_users.saturating_add(amount);
        } else {
            self.active_users = self.active_users.saturating_sub(amount);
        }
    }
}
