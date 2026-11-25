use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FossilSiteKind {
    #[default]
    Puzzle,
    PlaceableObject,
    Cash,
}

#[derive(Component)]
pub struct DigPit;

/// Supplies the puzzle requested by the scenario while there are unfound pieces.
#[derive(Component, PartialEq, Eq)]
pub struct RequestedFossilPuzzle(pub AssetId);

/// Starts a fresh fossil search with an optional named puzzle.
#[derive(Message)]
pub struct SetFossilPuzzleToFind {
    pub root: Entity,
    pub puzzle: Option<AssetId>,
}
