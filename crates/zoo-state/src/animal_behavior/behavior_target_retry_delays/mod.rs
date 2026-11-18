//! Per-actor rejection of a target across all task candidates.

use bevy::prelude::*;
use openzt2_game_data::world_definitions::behavior_selection_policy::BehaviorSelectionPolicy;

#[derive(Component, Default)]
pub struct BehaviorTargetRetryDelays {
    targets: std::collections::BTreeMap<Entity, RejectedBehaviorTarget>,
    completed_updates: u64,
    selection_update: u64,
    selection_epoch: u64,
}

struct RejectedBehaviorTarget {
    failures: u32,
    ready_at_update: u64,
    rejected_on_epoch: Option<u64>,
}

impl BehaviorTargetRetryDelays {
    pub fn from_snapshot(targets: impl IntoIterator<Item = (Entity, u32, u32)>) -> Self {
        Self {
            targets: targets
                .into_iter()
                .filter(|(_, failures, _)| *failures > 0)
                .map(|(target, failures, remaining_updates)| {
                    (
                        target,
                        RejectedBehaviorTarget {
                            failures,
                            ready_at_update: u64::from(remaining_updates),
                            rejected_on_epoch: None,
                        },
                    )
                })
                .collect(),
            ..Self::default()
        }
    }

    pub fn snapshot_state(&self) -> impl Iterator<Item = (Entity, u32, u32)> + '_ {
        self.targets.iter().filter_map(|(&target, rejected)| {
            (rejected.failures > 0).then(|| {
                let remaining = rejected
                    .ready_at_update
                    .saturating_sub(self.completed_updates);
                (
                    target,
                    rejected.failures,
                    u32::try_from(remaining).unwrap_or(u32::MAX),
                )
            })
        })
    }

    pub fn rejects(&self, target: Entity) -> bool {
        self.targets.get(&target).is_some_and(|rejected| {
            self.selection_update < rejected.ready_at_update
                || rejected.rejected_on_epoch == Some(self.selection_epoch)
        })
    }

    /// Selection uses the same update number on both sides of behavior execution.
    pub fn begin_candidate_selection_update(&mut self) {
        self.selection_epoch += 1;
        self.selection_update = self.completed_updates;
    }

    /// One eligible cognitive update, independent of elapsed simulation time.
    pub fn advance_cognitive_update(&mut self) {
        self.completed_updates += 1;
    }

    pub fn record_failure(&mut self, target: Entity, policy: BehaviorSelectionPolicy) {
        let rejected = self
            .targets
            .entry(target)
            .or_insert(RejectedBehaviorTarget {
                failures: 0,
                ready_at_update: self.completed_updates,
                rejected_on_epoch: None,
            });
        rejected.failures = rejected.failures.saturating_add(1);
        rejected.rejected_on_epoch = Some(self.selection_epoch);
        let delay = rejected
            .failures
            .saturating_mul(policy.rejected_target_retry_interval)
            .min(policy.rejected_target_retry_interval_cap);
        rejected.ready_at_update = self.completed_updates.saturating_add(u64::from(delay));
    }

    pub fn clear_successful_target(&mut self, target: Entity) {
        let rejects = self.rejects(target);
        if let Some(rejected) = self.targets.get_mut(&target) {
            rejected.failures = 0;
            rejected.ready_at_update = self.selection_update;
            rejected.rejected_on_epoch = rejects.then_some(self.selection_epoch);
        }
    }

    pub fn remove_destroyed_targets(&mut self, entities: &Query<()>) {
        self.targets.retain(|target, rejected| {
            (rejected.failures > 0 || rejected.rejected_on_epoch == Some(self.selection_epoch))
                && entities.contains(*target)
        });
    }
}
