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

pub fn update_behavior_target_retry(
    world: &mut World,
    actor: Entity,
    target: Entity,
    failure: Option<BehaviorSelectionPolicy>,
) {
    if world.get_entity(target).is_err() {
        return;
    }
    let Ok(entity) = world.get_entity(actor) else {
        return;
    };
    let existing = entity.get::<BehaviorRejections>().and_then(|records| {
        records.iter().find(|record| {
            world
                .get::<RejectedTarget>(*record)
                .is_some_and(|value| value.0 == target)
        })
    });
    if existing.is_none() && failure.is_none() {
        return;
    }
    if world.get::<BehaviorTargetRetryClock>(actor).is_none() {
        world
            .entity_mut(actor)
            .insert(BehaviorTargetRetryClock::default());
    }
    let clock = world
        .get::<BehaviorTargetRetryClock>(actor)
        .expect("actor has a retry clock");
    let epoch = clock.selection_epoch;
    let completed = clock.completed_updates;
    let selection_update = clock.selection_update;
    let was_rejected = existing
        .and_then(|record| world.get::<RetryDelay>(record))
        .is_some_and(|delay| delay.rejects(clock));

    if let Some(policy) = failure {
        let failures = existing
            .and_then(|record| world.get::<RetryDelay>(record))
            .map_or(1, |delay| delay.failures.saturating_add(1));
        let remaining = failures
            .saturating_mul(policy.rejected_target_retry_interval)
            .min(policy.rejected_target_retry_interval_cap);
        let value = RetryDelay {
            failures,
            ready_at_update: completed.saturating_add(u64::from(remaining)),
            rejected_on_epoch: Some(epoch),
        };
        if let Some(record) = existing {
            *world
                .get_mut::<RetryDelay>(record)
                .expect("target has a retry delay") = value;
        } else {
            world.spawn((RejectedBy(actor), RejectedTarget(target), value));
        }
    } else if let Some(record) = existing {
        if was_rejected {
            // Selectors on either side of this outcome use the same update.
            *world
                .get_mut::<RetryDelay>(record)
                .expect("target has a retry delay") = RetryDelay {
                failures: 0,
                ready_at_update: selection_update,
                rejected_on_epoch: Some(epoch),
            };
        } else {
            world.despawn(record);
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
