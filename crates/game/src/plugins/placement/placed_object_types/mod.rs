use bevy::prelude::*;
use openzt2_game_data::{world_definitions::object_placement::EntrancePurpose, AssetId};

/// Stable definition identity attached to an object governed by placement.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacedObjectDefinitionReference {
    pub definition: AssetId,
}

/// Authoritative occupied grid origin and authored orientation for a placed object.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacedObjectFootprintOccupancy {
    pub origin: IVec2,
    pub eighth_turns: u8,
}

/// Opt-in for objects whose physics motion changes their occupied placement cells.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PhysicsMovedPlacedObjectFootprint;

/// One authored object entrance projected into the Bevy world.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PlacedObjectAuthoredEntrance {
    pub local_position: Vec3,
    pub local_forward: Vec3,
    pub purpose: EntrancePurpose,
}
