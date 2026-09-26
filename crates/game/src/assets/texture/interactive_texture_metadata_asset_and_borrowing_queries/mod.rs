//! Canonical interactive-image metadata paired with its cursor-atlas image handle.

use bevy::prelude::*;
use openzt2_game_data::image::{
    ImageAlphaHitMask, ImageCursorAtlasEntry, InteractiveImageMetadata,
};

#[derive(Asset, TypePath, Clone, Debug)]
pub struct InteractiveTextureMetadataAsset {
    pub(in crate::assets::texture) metadata: InteractiveImageMetadata,
    pub(in crate::assets::texture) cursor_atlas_image: Option<Handle<Image>>,
}

impl InteractiveTextureMetadataAsset {
    #[must_use]
    pub fn alpha_hit_mask_dimensions(&self) -> Option<UVec2> {
        self.metadata
            .alpha_hit_mask
            .as_ref()
            .map(|mask| UVec2::new(mask.width, mask.height))
    }

    #[must_use]
    pub fn alpha_hit_mask_contains_pixel(&self, x: u32, y: u32) -> bool {
        let Some(ImageAlphaHitMask {
            width,
            height,
            bits,
        }) = &self.metadata.alpha_hit_mask
        else {
            return false;
        };
        if x >= *width || y >= *height {
            return false;
        }
        usize::try_from(y)
            .ok()
            .and_then(|y| {
                usize::try_from(*width)
                    .ok()
                    .and_then(|width| y.checked_mul(width))
            })
            .and_then(|row_start| {
                usize::try_from(x)
                    .ok()
                    .and_then(|x| row_start.checked_add(x))
            })
            .and_then(|index| {
                bits.get(index / 8)
                    .map(|byte| byte & (1 << (index % 8)) != 0)
            })
            .unwrap_or(false)
    }

    #[must_use]
    pub fn cursor_atlas_image_and_nearest_entry_for_window_scale_factor(
        &self,
        window_scale_factor: f32,
    ) -> Option<(&Handle<Image>, ImageCursorAtlasEntry)> {
        let cursor_atlas = self.metadata.cursor_atlas.as_ref()?;
        let target_extent = (32.0 * window_scale_factor).round().max(1.0) as u32;
        let nearest_entry = cursor_atlas
            .entries
            .iter()
            .copied()
            .min_by_key(|entry| entry.size[0].max(entry.size[1]).abs_diff(target_extent))?;
        Some((self.cursor_atlas_image.as_ref()?, nearest_entry))
    }
}
