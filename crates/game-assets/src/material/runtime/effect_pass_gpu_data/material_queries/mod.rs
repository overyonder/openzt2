//! Material input dependencies and texture queries.

use super::{
    EffectPassMaterial, EffectPassTextureAssets, EFFECT_ENVIRONMENT_REGISTER_BUFFER,
    FIXED_FUNCTION_WORLD_LIGHT_BUFFER,
};
use bevy::{
    prelude::*,
    render::render_resource::{BlendState, ColorWrites, CompareFunction, StencilState},
};

/// Authored graphics state available to renderers that share an effect pass's
/// material without using the PBR mesh draw command.
pub struct EffectPassDrawState {
    pub blend: Option<BlendState>,
    pub depth_write_enabled: bool,
    pub depth_compare: CompareFunction,
    pub color_writes: ColorWrites,
    pub stencil: StencilState,
    pub stencil_reference: u32,
}

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

    pub fn draw_state(&self) -> EffectPassDrawState {
        let key = &self.pipeline_specialization_key;
        EffectPassDrawState {
            blend: key.blend_state,
            depth_write_enabled: key.depth_write_enabled,
            depth_compare: key.depth_compare,
            color_writes: key.color_writes,
            stencil: key.stencil.clone(),
            stencil_reference: key.stencil_reference,
        }
    }

    pub fn bound_particle_texture(&self) -> Option<&Handle<Image>> {
        self.texture_assets
            .ordered_texture_assets()
            .into_iter()
            .flatten()
            .next()
    }

    pub(crate) fn hanabi_blend_mode(&self) -> Option<bevy_hanabi::AlphaMode> {
        use bevy_hanabi::AlphaMode;

        let blend = self.pipeline_specialization_key.blend_state?;
        [
            AlphaMode::Blend,
            AlphaMode::Premultiply,
            AlphaMode::Add,
            AlphaMode::Multiply,
        ]
        .into_iter()
        .find(|&mode| BlendState::from(mode) == blend)
    }

    pub fn uses_binned_opaque_phase(&self) -> bool {
        self.pipeline_specialization_key.independent_opaque
    }

    pub fn retain_authored_draw_order(&mut self) {
        self.pipeline_specialization_key.independent_opaque = false;
    }

    pub fn has_opaque_blend_state(&self) -> bool {
        self.pipeline_specialization_key.blend_state.is_none()
    }
}
