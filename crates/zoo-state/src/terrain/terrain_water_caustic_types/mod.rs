//! Animation state for the two projected caustic textures.

use bevy::prelude::*;

#[derive(Component, Clone, Debug)]
pub struct TerrainWaterCausticAnimation {
    pub bottom_phase: Vec2,
    pub top_phase: Vec2,
    pub bottom_rock: TerrainWaterCausticRock,
    pub top_rock: TerrainWaterCausticRock,
}

#[derive(Clone, Copy, Debug)]
pub struct TerrainWaterCausticRock {
    pub phase: f32,
    pub rate: f32,
    pub distance: f32,
}

/// One camera-specific draw of a ground caustic mesh.
#[derive(Component, Clone, Copy)]
pub struct TerrainWaterCausticPass {
    pub water_owner: Entity,
    pub render_view: Entity,
}

#[derive(Component)]
pub struct TerrainWaterCausticProjectionRevision(pub u64);

/// Derived from the physical presentation's authored render hierarchy.
#[derive(Component, Clone, Copy, PartialEq)]
pub struct TerrainWaterCausticSceneTarget {
    pub water_owner: Entity,
}

#[derive(Component, Clone, Copy)]
pub struct TerrainWaterCausticScenePass {
    pub physical_presentation: Entity,
    pub source_primitive: Entity,
    pub water_owner: Entity,
    pub render_view: Entity,
}
