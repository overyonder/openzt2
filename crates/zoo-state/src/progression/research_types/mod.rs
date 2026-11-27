use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(ResearchDurationOverride)]
pub struct ResearchProjectAvailability {
    pub item: AssetId,
    pub available: bool,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ResearchAvailabilityUnlockCountdown {
    pub item: AssetId,
    pub timer: Timer,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ResearchProject {
    pub definition: AssetId,
    pub timer: Timer,
}

impl ResearchProject {
    pub fn new(definition: AssetId, duration: Duration) -> Self {
        Self {
            definition,
            timer: Timer::new(duration, TimerMode::Once),
        }
    }

    pub fn from_saved_ticks(
        definition: AssetId,
        step: Duration,
        required: u64,
        elapsed: u64,
    ) -> Self {
        let mut project = Self::new(definition, fixed_tick_duration(step, required));
        project
            .timer
            .set_elapsed(fixed_tick_duration(step, elapsed.min(required)));
        project
    }

    pub fn saved_ticks(&self, step: Duration) -> (u64, u64) {
        let step = step.as_nanos().max(1);
        let required = self.timer.duration().as_nanos().div_ceil(step);
        let elapsed = self.timer.elapsed().as_nanos() / step;
        (
            u64::try_from(required).unwrap_or(u64::MAX),
            u64::try_from(elapsed).unwrap_or(u64::MAX),
        )
    }
}

impl ResearchAvailabilityUnlockCountdown {
    pub fn from_saved_ticks(item: AssetId, step: Duration, remaining: u64) -> Self {
        Self {
            item,
            timer: Timer::new(fixed_tick_duration(step, remaining), TimerMode::Once),
        }
    }

    pub fn remaining_ticks(&self, step: Duration) -> u64 {
        let remaining = self
            .timer
            .remaining()
            .as_nanos()
            .div_ceil(step.as_nanos().max(1));
        u64::try_from(remaining).unwrap_or(u64::MAX)
    }
}

pub(super) fn fixed_tick_duration(step: Duration, ticks: u64) -> Duration {
    let nanos = step.as_nanos().saturating_mul(u128::from(ticks));
    u64::try_from(nanos / 1_000_000_000).map_or(Duration::MAX, |seconds| {
        Duration::new(seconds, (nanos % 1_000_000_000) as u32)
    })
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResearchProjectPaused;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = ResearchItemPayments)]
pub struct PendingResearchProjectPayment {
    pub item: Entity,
}

#[derive(Component)]
#[relationship_target(relationship = PendingResearchProjectPayment, linked_spawn)]
pub struct ResearchItemPayments(Entity);

impl ResearchItemPayments {
    pub fn pending_payment(&self) -> Entity {
        *self.collection()
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct ResearchDurationOverride(pub Option<f32>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchProjectPersistentId(pub PersistentId);

#[derive(Component)]
#[relationship(relationship_target = WorldResearchItemMembers)]
pub struct ResearchItemInWorld(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ResearchItemInWorld, linked_spawn)]
pub struct WorldResearchItemMembers(Vec<Entity>);

/// Lookup for the authored research items of one loaded world.
#[derive(Component, Debug, Default, PartialEq, Eq)]
pub struct WorldResearchItems {
    pub definitions: BTreeMap<AssetId, Entity>,
    pub unlocks: BTreeMap<AssetId, Vec<Entity>>,
}

impl WorldResearchItems {
    pub fn unlocking(&self, target: AssetId) -> impl Iterator<Item = Entity> + '_ {
        self.unlocks.get(&target).into_iter().flatten().copied()
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartResearchProjectRequest {
    pub item: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchProjectCompleted {
    pub root: Entity,
    pub project: Entity,
    pub definition: AssetId,
}

/// Overrides how long one research project takes, from a scenario script.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct SetResearchDuration {
    pub definition: AssetId,
    pub seconds: f32,
}
