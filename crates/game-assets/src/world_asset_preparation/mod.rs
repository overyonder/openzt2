//! Operation-scoped demand for world metadata before a world is loaded.

use bevy::prelude::Component;

#[derive(Component)]
pub struct WorldAssetPreparation;

#[cfg(test)]
mod tests;
