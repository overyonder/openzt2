use bevy::prelude::*;

use crate::world_spawn::persistent_id_types::PersistentId;
use crate::world_spawn::persistent_id_types::PersistentIdError;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssignPersistentId {
    pub entity: Entity,
    pub root: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistentIdAssigned {
    pub entity: Entity,
    pub id: PersistentId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistentIdAssignmentFailed {
    pub entity: Entity,
    pub reason: PersistentIdError,
}
