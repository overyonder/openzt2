//! Per-actor rejection of a target across all task candidates.

use bevy::prelude::*;
use openzt2_game_data::world_definitions::behavior_selection_policy::BehaviorSelectionPolicy;

#[derive(Component, Default)]
pub struct BehaviorTargetRetryDelays {
    targets: Vec<RejectedBehaviorTarget>,
}

struct RejectedBehaviorTarget {
    target: Entity,
    failures: u32,
    remaining_updates: u32,
}

impl BehaviorTargetRetryDelays {
    pub fn rejects(&self, target: Entity) -> bool {
        self.targets
            .iter()
            .any(|rejected| rejected.target == target && rejected.remaining_updates > 0)
    }

    /// One running or intentionally idle cognitive update, rather than elapsed time.
    pub fn advance_cognitive_update(&mut self) {
        for rejected in &mut self.targets {
            rejected.remaining_updates = rejected.remaining_updates.saturating_sub(1);
        }
    }

    pub fn record_failure(&mut self, target: Entity, policy: BehaviorSelectionPolicy) {
        let index = self
            .targets
            .iter()
            .position(|rejected| rejected.target == target);
        let rejected = if let Some(index) = index {
            &mut self.targets[index]
        } else {
            self.targets.push(RejectedBehaviorTarget {
                target,
                failures: 0,
                remaining_updates: 0,
            });
            self.targets.last_mut().expect("inserted target")
        };
        rejected.failures = rejected.failures.saturating_add(1);
        rejected.remaining_updates = rejected
            .failures
            .saturating_mul(policy.rejected_target_retry_interval)
            .min(policy.rejected_target_retry_interval_cap);
    }

    pub fn clear_successful_target(&mut self, target: Entity) {
        self.targets.retain(|rejected| rejected.target != target);
    }

    pub fn remove_destroyed_targets(&mut self, entities: &Query<()>) {
        self.targets
            .retain(|rejected| entities.contains(rejected.target));
    }
}
