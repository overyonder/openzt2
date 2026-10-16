use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorldEntityIsCrated;

#[derive(Component, Debug, Clone, Copy)]
pub struct ContainedCratedAnimal(pub Entity);

#[derive(Component, Debug, Clone, Copy)]
pub struct AnimalCrate(pub Entity);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoveWorldEntityFromCrate(pub Entity);
