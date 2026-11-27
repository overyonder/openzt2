//! Material input dependencies and texture queries.

use super::{
    EffectPassMaterial, EffectPassTextureAssets, EFFECT_ENVIRONMENT_REGISTER_BUFFER,
    FIXED_FUNCTION_WORLD_LIGHT_BUFFER,
};
use bevy::prelude::*;

impl EffectPassTextureAssets {
    pub(in crate::material::runtime) fn from_ordered_texture_assets(
        texture_assets: [Option<Handle<Image>>; 8],
    ) -> Self {
        let [texture0, texture1, texture2, texture3, texture4, texture5, texture6, texture7] =
            texture_assets;
        Self {
            texture0,
            texture1,
            texture2,
            texture3,
            texture4,
            texture5,
            texture6,
            texture7,
            fixed_function_world_lights: FIXED_FUNCTION_WORLD_LIGHT_BUFFER,
            environment_registers: EFFECT_ENVIRONMENT_REGISTER_BUFFER,
        }
    }

    fn present_texture_asset_count(&self) -> usize {
        [
            &self.texture0,
            &self.texture1,
            &self.texture2,
            &self.texture3,
            &self.texture4,
            &self.texture5,
            &self.texture6,
            &self.texture7,
        ]
        .into_iter()
        .filter(|texture| texture.is_some())
        .count()
    }

    pub(in crate::material::runtime) const fn ordered_texture_assets(
        &self,
    ) -> [Option<&Handle<Image>>; 8] {
        [
            self.texture0.as_ref(),
            self.texture1.as_ref(),
            self.texture2.as_ref(),
            self.texture3.as_ref(),
            self.texture4.as_ref(),
            self.texture5.as_ref(),
            self.texture6.as_ref(),
            self.texture7.as_ref(),
        ]
    }
}

impl EffectPassMaterial {
    pub(in crate::material) fn present_texture_asset_count(&self) -> usize {
        self.texture_assets.present_texture_asset_count()
    }

    pub fn has_opaque_blend_state(&self) -> bool {
        self.pipeline_specialization_key.blend_state.is_none()
    }
}
