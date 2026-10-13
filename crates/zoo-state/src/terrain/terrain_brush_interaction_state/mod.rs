use bevy::prelude::*;

/// Per-frame pointer samples retained only until one terrain transaction has
/// been prepared. This is interaction history, not a second terrain model.
#[derive(Component, Debug, Default)]
pub struct TerrainBrushPath(pub Vec<TerrainBrushDab>);

#[derive(Debug, Clone, Copy)]
pub struct TerrainBrushDab {
    pub center: Vec2,
    pub seconds: f32,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct ActiveTerrainBrush {
    pub kind: crate::terrain::terrain_brush_types::TerrainBrushKind,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct PendingTerrainBrushCommit;
