use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct TerrainCollisionRevision(pub u64);

/// Marks a chunk whose edited collision surface still needs to be handed to
/// Avian. The marker makes deferral explicit when the per-tick rebuild budget
/// is exhausted; scanning is bounded and no revision is lost.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(super) struct TerrainCollisionPending;

/// The centred Avian height-field child owned by one rendered terrain chunk.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(super) struct TerrainChunkCollisionSurface;

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TerrainCollisionBudget {
    pub max_rebuilds_per_tick: usize,
}

impl Default for TerrainCollisionBudget {
    fn default() -> Self {
        Self {
            max_rebuilds_per_tick: 1,
        }
    }
}
