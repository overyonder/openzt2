//! Requested animal spawn identity, outcome, and failure contracts.

use bevy::prelude::*;
use openzt2_game_data::{AssetId, species::Sex};

use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdError;

use super::types::Parents;

#[derive(Message, Debug, Clone)]
pub(super) struct SpawnAnimalRequest {
    pub(super) operation: Entity,
    pub(super) species: AssetId,
    pub(super) variant: Option<AssetId>,
    pub(super) sex: Option<Sex>,
    pub(super) world: Entity,
    pub(super) habitat: Option<Entity>,
    pub(super) transform: Transform,
    pub(super) parents: Parents,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AnimalSpawned {
    pub(super) operation: Entity,
    pub(super) animal: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AnimalSpawnRejected {
    pub(super) operation: Entity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AnimalSpawnFailure {
    MissingSpecies,
    InvalidVariant,
    InvalidSex,
    InvalidHabitat,
    PersistentId(PersistentIdError),
}
