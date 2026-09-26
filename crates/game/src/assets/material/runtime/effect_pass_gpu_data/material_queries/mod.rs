//! Material input dependencies and texture queries.

use super::super::resolved_effect_transform_bindings::{
    FOG_INPUT, LIGHT_INPUT, MODEL_INPUT, VIEW_INPUT, WIND_INPUT,
};
use super::{EffectPassMaterial, EffectPassTextureAssets, FIXED_FUNCTION_WORLD_LIGHT_BUFFER};
use bevy::prelude::*;

impl EffectPassTextureAssets {
    pub(in crate::assets::material::runtime) fn from_ordered_texture_assets(
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

    pub(in crate::assets::material::runtime) const fn ordered_texture_assets(
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
    pub(crate) fn programmable_inputs_changed(
        &self,
        model: bool,
        view: bool,
        wind: bool,
        light: bool,
        fog: bool,
    ) -> bool {
        self.parameter_input_dependencies
            & ((u8::from(model) * MODEL_INPUT)
                | (u8::from(view) * VIEW_INPUT)
                | (u8::from(wind) * WIND_INPUT)
                | (u8::from(light) * LIGHT_INPUT)
                | (u8::from(fog) * FOG_INPUT))
            != 0
    }

    pub(crate) fn has_programmable_effect_parameter_bindings(&self) -> bool {
        !self.dynamic_parameter_register_bindings.is_empty()
    }

    /// Only model/view inputs require a draw-specific register bank. Wind and
    /// world lighting have identical values for every instance of this asset.
    pub(crate) const fn requires_draw_specific_shader_registers(&self) -> bool {
        self.parameter_input_dependencies & (MODEL_INPUT | VIEW_INPUT | FOG_INPUT) != 0
    }

    /// Model-only registers remain valid in reflection/refraction cameras;
    /// only camera-dependent inputs require a separate view projection.
    pub(crate) const fn requires_view_specific_shader_registers(&self) -> bool {
        self.parameter_input_dependencies & (VIEW_INPUT | FOG_INPUT) != 0
    }

    pub(in crate::assets::material) fn present_texture_asset_count(&self) -> usize {
        self.texture_assets.present_texture_asset_count()
    }

    pub(crate) fn has_opaque_blend_state(&self) -> bool {
        self.pipeline_specialization_key.blend_state.is_none()
    }
}
