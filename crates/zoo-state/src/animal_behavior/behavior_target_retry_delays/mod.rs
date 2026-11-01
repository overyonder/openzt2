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
    reject_for_current_update: bool,
}

impl BehaviorTargetRetryDelays {
    pub fn from_snapshot(targets: impl IntoIterator<Item = (Entity, u32, u32)>) -> Self {
        Self {
            targets: targets
                .into_iter()
                .map(
                    |(target, failures, remaining_updates)| RejectedBehaviorTarget {
                        target,
                        failures,
                        remaining_updates,
                        reject_for_current_update: remaining_updates > 0,
                    },
                )
                .collect(),
        }
    }

    pub fn snapshot_state(&self) -> impl Iterator<Item = (Entity, u32, u32)> + '_ {
        self.targets
            .iter()
            .filter(|rejected| rejected.failures > 0)
            .map(|rejected| {
                (
                    rejected.target,
                    rejected.failures,
                    rejected.remaining_updates,
                )
            })
    }

    pub fn rejects(&self, target: Entity) -> bool {
        self.targets
            .iter()
            .any(|rejected| rejected.target == target && rejected.reject_for_current_update)
    }

    /// Selection in one simulation update observes the same rejection window,
    /// even when its behavior executor runs earlier than another actor's selector.
    pub fn begin_candidate_selection_update(&mut self) {
        self.targets.retain(|rejected| rejected.failures > 0);
        for rejected in &mut self.targets {
            rejected.reject_for_current_update = rejected.remaining_updates > 0;
        }
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
                reject_for_current_update: true,
            });
            self.targets.last_mut().expect("inserted target")
        };
        rejected.failures = rejected.failures.saturating_add(1);
        rejected.reject_for_current_update = true;
        rejected.remaining_updates = rejected
            .failures
            .saturating_mul(policy.rejected_target_retry_interval)
            .min(policy.rejected_target_retry_interval_cap);
    }

    pub fn clear_successful_target(&mut self, target: Entity) {
        if let Some(rejected) = self
            .targets
            .iter_mut()
            .find(|rejected| rejected.target == target)
        {
            rejected.failures = 0;
            rejected.remaining_updates = 0;
        }
    }

    pub fn remove_destroyed_targets(&mut self, entities: &Query<()>) {
        self.targets
            .retain(|rejected| entities.contains(rejected.target));
    }
}
