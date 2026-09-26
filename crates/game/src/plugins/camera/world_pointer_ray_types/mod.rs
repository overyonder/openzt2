use bevy::{math::Ray3d, prelude::*};

#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct WorldPointerRay(pub(crate) Option<Ray3d>);
