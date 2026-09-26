use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct PrefabAmbientLightContribution {
    pub(crate) color_linear: Vec3,
    pub(crate) intensity: f32,
}
