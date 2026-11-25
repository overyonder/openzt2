use bevy::prelude::*;

/// Topology cells whose containing habitat identity changed in a loaded world.
#[derive(Message)]
pub struct HabitatCellOwnershipChanged {
    pub root: Entity,
    pub cells: Vec<IVec2>,
}
