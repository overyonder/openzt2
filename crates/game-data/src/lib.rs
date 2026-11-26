//! Shared authored-data values used by asset loading and gameplay.
//!
//! These are ordinary owned Rust values suitable for standard text formats.
//! This crate does not define archive access, renderer mirrors, loaders, or
//! live game state.

use serde::{Deserialize, Serialize};

mod asset_identifier_operations;

pub mod animation;
pub mod audio;
pub mod behavior;
pub mod image;
pub mod localization;
pub mod particle;
pub mod scene_prefab;
pub mod species;
pub mod terrain;
pub mod ui_document;
pub mod world_definitions;
pub mod world_scenario;

/// Stable identity derived from an authored canonical key.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(transparent)]
pub struct AssetId(pub [u8; 16]);

/// Effective locomotion multipliers from an entity definition.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct CaffeineFactors {
    pub caffeinated: f32,
    pub decaffeinated: f32,
}

impl Default for CaffeineFactors {
    fn default() -> Self {
        Self {
            caffeinated: 1.0,
            decaffeinated: 1.0,
        }
    }
}
