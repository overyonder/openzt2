use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorldEntityIsCrated;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoveWorldEntityFromCrate(pub Entity);
