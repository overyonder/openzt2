use bevy::prelude::*;

#[derive(Component, Clone, Copy, Debug)]
pub struct SelectedNaturalWaterTarget {
    pub region: Entity,
    pub water: Vec3,
    pub approach: Vec3,
    pub kind: NaturalWaterTargetKind,
}

#[derive(Clone, Copy, Debug)]
pub enum NaturalWaterTargetKind {
    Shore,
    Surface { depth: f32 },
}
