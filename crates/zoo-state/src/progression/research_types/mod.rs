use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use std::collections::BTreeMap;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(ResearchPurchaseState, ResearchDurationOverride)]
pub struct ResearchProjectAvailability {
    pub item: AssetId,
    pub available: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchAvailabilityUnlockCountdown {
    pub item: AssetId,
    pub remaining_ticks: u64,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchProject {
    pub definition: AssetId,
    pub elapsed_ticks: u64,
    pub required_ticks: u64,
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

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ResearchPurchaseState {
    #[default]
    Idle,
    AwaitingPayment(Entity),
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

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchProjectCompletionReady {
    pub project: Entity,
    pub definition: AssetId,
}

/// Overrides how long one research project takes, from a scenario script.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct SetResearchDuration {
    pub definition: AssetId,
    pub seconds: f32,
}
