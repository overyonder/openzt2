use bevy::prelude::*;

/// Marks a chunk whose edited collision surface still needs to be handed to
/// Avian. The marker makes deferral explicit when the per-tick rebuild budget
/// is exhausted.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TerrainCollisionPending;

/// The centred Avian height-field child owned by one rendered terrain chunk.
/// Its change tick tracks when the terrain last rebuilt the collider.
#[allow(
    clippy::redundant_pub_crate,
    reason = "placement queries need sibling-owner access; the terrain collider marker remains a crate-internal API"
)]
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TerrainChunkCollisionSurface;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainCollisionBudget {
    pub max_rebuilds_per_tick: usize,
}

impl Default for TerrainCollisionBudget {
    fn default() -> Self {
        Self {
            max_rebuilds_per_tick: 1,
        }
    }
}
