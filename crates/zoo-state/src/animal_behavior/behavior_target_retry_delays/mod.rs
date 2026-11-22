//! Cognitive retry delays owned by the actor and the rejected target.

use bevy::ecs::{relationship::RelationshipTarget, system::SystemParam};
use bevy::prelude::*;
use openzt2_game_data::world_definitions::behavior_selection_policy::BehaviorSelectionPolicy;

#[derive(Component, Default)]
pub struct BehaviorTargetRetryClock {
    completed_updates: u64,
    selection_update: u64,
    selection_epoch: u64,
}

impl BehaviorTargetRetryClock {
    pub fn begin_candidate_selection_update(&mut self) {
        self.selection_epoch = self.selection_epoch.saturating_add(1);
        self.selection_update = self.completed_updates;
    }

    pub fn advance_cognitive_update(&mut self) {
        self.completed_updates = self.completed_updates.saturating_add(1);
    }
}

#[derive(Component)]
#[relationship(relationship_target = BehaviorRejections)]
pub struct RejectedBy(Entity);

#[derive(Component)]
#[relationship_target(relationship = RejectedBy, linked_spawn)]
pub struct BehaviorRejections(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = BehaviorRejectionsOfTarget)]
struct RejectedTarget(Entity);

#[derive(Component)]
#[relationship_target(relationship = RejectedTarget, linked_spawn)]
struct BehaviorRejectionsOfTarget(Vec<Entity>);

#[derive(Component)]
pub struct RetryDelay {
    failures: u32,
    ready_at_update: u64,
    rejected_on_epoch: Option<u64>,
}

impl RetryDelay {
    fn rejects(&self, clock: &BehaviorTargetRetryClock) -> bool {
        clock.selection_update < self.ready_at_update
            || self.rejected_on_epoch == Some(clock.selection_epoch)
    }
}

#[derive(SystemParam)]
pub struct BehaviorTargetRejections<'w, 's> {
    actors: Query<
        'w,
        's,
        (
            &'static BehaviorTargetRetryClock,
            &'static BehaviorRejections,
        ),
    >,
    delays: Query<'w, 's, (&'static RejectedTarget, &'static RetryDelay)>,
}

impl BehaviorTargetRejections<'_, '_> {
    pub fn rejects(&self, actor: Entity, target: Entity) -> bool {
        let Ok((clock, rejections)) = self.actors.get(actor) else {
            return false;
        };
        rejections.iter().any(|record| {
            self.delays
                .get(record)
                .is_ok_and(|(rejected, delay)| rejected.0 == target && delay.rejects(clock))
        })
    }

    pub fn snapshot(&self, actor: Entity) -> impl Iterator<Item = (Entity, u32, u32)> + '_ {
        self.actors
            .get(actor)
            .ok()
            .into_iter()
            .flat_map(move |(clock, records)| {
                records.iter().filter_map(move |record| {
                    let (target, delay) = self.delays.get(record).ok()?;
                    let remaining = delay
                        .ready_at_update
                        .saturating_sub(clock.completed_updates);
                    (delay.failures > 0).then_some((
                        target.0,
                        delay.failures,
                        u32::try_from(remaining).unwrap_or(u32::MAX),
                    ))
                })
            })
    }
}

pub fn restore_behavior_target_retry(
    world: &mut World,
    actor: Entity,
    target: Entity,
    failures: u32,
    remaining: u32,
) {
    if failures == 0 || actor == target || world.get_entity(target).is_err() {
        return;
    }
    let Ok(mut entity) = world.get_entity_mut(actor) else {
        return;
    };
    let completed_updates = entity
        .entry::<BehaviorTargetRetryClock>()
        .or_default()
        .get()
        .completed_updates;
    world.spawn((
        RejectedBy(actor),
        RejectedTarget(target),
        RetryDelay {
            failures,
            ready_at_update: completed_updates.saturating_add(u64::from(remaining)),
            rejected_on_epoch: None,
        },
    ));
}

#[derive(SystemParam)]
pub struct BehaviorTargetRetryOutcomes<'w, 's> {
    actors: Query<'w, 's, &'static BehaviorRejections>,
    targets: Query<'w, 's, &'static RejectedTarget>,
    delays: Query<'w, 's, &'static mut RetryDelay>,
}

impl BehaviorTargetRetryOutcomes<'_, '_> {
    pub fn settle(
        &mut self,
        commands: &mut Commands,
        actor: Entity,
        target: Entity,
        clock: Option<&BehaviorTargetRetryClock>,
        failure: Option<BehaviorSelectionPolicy>,
    ) {
        let existing = self.actors.get(actor).ok().and_then(|records| {
            records.iter().find(|record| {
                self.targets
                    .get(*record)
                    .is_ok_and(|value| value.0 == target)
            })
        });
        let default_clock = BehaviorTargetRetryClock::default();
        let current_clock = clock.unwrap_or(&default_clock);
        let epoch = current_clock.selection_epoch;
        let delay = existing.and_then(|record| self.delays.get_mut(record).ok());
        if let Some(policy) = failure {
            let failures = delay
                .as_ref()
                .map_or(1, |delay| delay.failures.saturating_add(1));
            let remaining = failures
                .saturating_mul(policy.rejected_target_retry_interval)
                .min(policy.rejected_target_retry_interval_cap);
            let next = RetryDelay {
                failures,
                ready_at_update: current_clock
                    .completed_updates
                    .saturating_add(u64::from(remaining)),
                rejected_on_epoch: Some(epoch),
            };
            if clock.is_none() {
                commands.entity(actor).insert(default_clock);
            }
            if let Some(mut delay) = delay {
                *delay = next;
            } else {
                commands.spawn((RejectedBy(actor), RejectedTarget(target), next));
            }
        } else if let (Some(record), Some(mut delay)) = (existing, delay) {
            if delay.rejects(current_clock) {
                // Both selectors see the same cognitive update.
                *delay = RetryDelay {
                    failures: 0,
                    ready_at_update: current_clock.selection_update,
                    rejected_on_epoch: Some(epoch),
                };
            } else {
                commands.entity(record).despawn();
            }
        }
    }
}

pub fn reap_cleared_behavior_target_retries(
    records: Query<(Entity, &RetryDelay), Changed<RetryDelay>>,
    mut commands: Commands,
) {
    for (record, delay) in &records {
        if delay.failures == 0 {
            commands.entity(record).despawn();
        }
    }
}
