use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct PrefabAmbientLightContribution {
    pub color_linear: Vec3,
    pub intensity: f32,
}
