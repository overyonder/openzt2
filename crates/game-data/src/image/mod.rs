//! Authored interaction data stored only beside images that need it.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InteractiveImageMetadata {
    pub alpha_hit_mask: Option<ImageAlphaHitMask>,
    pub cursor_atlas: Option<ImageCursorAtlas>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ImageAlphaHitMask {
    pub width: u32,
    pub height: u32,
    pub bits: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ImageCursorAtlas {
    pub atlas_path: String,
    pub entries: Vec<ImageCursorAtlasEntry>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct ImageCursorAtlasEntry {
    pub origin: [u32; 2],
    pub size: [u32; 2],
    pub hotspot: [u16; 2],
}
