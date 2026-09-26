use bevy::prelude::*;

/// Per-frame pointer samples retained only until one terrain transaction has
/// been prepared. This is interaction history, not a second terrain model.
#[derive(Component, Debug, Default)]
pub(crate) struct TerrainBrushPath(pub(crate) Vec<TerrainBrushDab>);

#[derive(Debug, Clone, Copy)]
pub(crate) struct TerrainBrushDab {
    pub(crate) center: Vec2,
    pub(crate) seconds: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct ActiveTerrainBrush;

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct PendingTerrainBrushCommit;
