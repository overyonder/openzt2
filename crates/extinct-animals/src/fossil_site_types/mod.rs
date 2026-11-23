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

/// Restricts the fossil search to one puzzle while the request is active.
#[derive(Component)]
pub struct RequestedFossilPuzzle(pub AssetId);
