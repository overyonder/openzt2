//! Observable animal birth and release operation contracts.

use bevy::prelude::*;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnimalBorn {
    pub(crate) child: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ReleaseAnimal(pub(super) Entity);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnimalReleased;
