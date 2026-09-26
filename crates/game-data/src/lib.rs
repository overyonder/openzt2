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
