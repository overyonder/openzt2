use bevy::prelude::*;

pub const PRIMARY_EFFECT_PASS_RENDER_VIEW_LAYER: usize = 5;
pub const WATER_REFRACTION_EFFECT_PASS_RENDER_VIEW_LAYER: usize = 6;
pub const WATER_REFLECTION_EFFECT_PASS_RENDER_VIEW_LAYER: usize = 7;

#[derive(Component, Clone, Copy)]
pub struct EffectPassMaterialRenderView {
    pub render_layer: usize,
}

// Each draw needs its camera, lights and render-view transforms together.
