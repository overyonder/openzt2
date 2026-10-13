//! Observable animal birth and release operation contracts.

use bevy::prelude::*;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimalBorn {
    pub child: Entity,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseAnimal(pub Entity);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimalReleased;
