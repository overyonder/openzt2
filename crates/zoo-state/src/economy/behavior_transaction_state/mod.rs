//! In-flight account settlement and named-chain position for a behavior action.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component)]
pub struct BehaviorTransactionPayment {
    pub actor: Entity,
    pub facility: Entity,
    pub transaction: AssetId,
}

/// Usage belongs to the account's live entity; purchase and departure update it.
#[derive(Component, Default)]
pub struct EconomyUsage {
    active_users: i32,
    total_users: i32,
}

impl EconomyUsage {
    pub fn from_counts(active_users: i32, total_users: i32) -> Self {
        Self {
            active_users,
            total_users,
        }
    }

    pub fn counts(&self) -> [i32; 2] {
        [self.active_users, self.total_users]
    }

    pub fn apply_user_change(&mut self, amount: i32, adding: bool) {
        if adding {
            self.active_users = self.active_users.saturating_add(amount);
            self.total_users = self.total_users.saturating_add(amount);
        } else {
            self.active_users = self.active_users.saturating_sub(amount);
        }
    }
}
