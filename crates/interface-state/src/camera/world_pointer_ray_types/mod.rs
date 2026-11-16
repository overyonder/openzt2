use bevy::prelude::*;

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct WorldPointerRay(pub Option<Ray3d>);
