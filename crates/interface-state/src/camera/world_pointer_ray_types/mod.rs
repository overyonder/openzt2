use bevy::{math::Ray3d, prelude::*};

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct WorldPointerRay(pub Option<Ray3d>);
