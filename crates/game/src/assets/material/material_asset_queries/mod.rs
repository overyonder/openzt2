use bevy::prelude::*;

use super::{
    material_asset_types::MaterialAsset, runtime::effect_pass_gpu_data::EffectPassMaterial,
};
use crate::plugins::settings::graphics_settings_types::GraphicsEffectQuality;

impl MaterialAsset {
    pub(crate) fn evaluated_d3d9_effect(&self) -> &d3d9_effects::effect_types::EvaluatedD3d9Effect {
        &self.evaluated_d3d9_effect
    }

    pub(crate) fn evaluated_pass_material_assets_for_effect_quality(
        &self,
        effect_quality: GraphicsEffectQuality,
    ) -> &[Handle<EffectPassMaterial>] {
        let wanted_quality = match effect_quality {
            GraphicsEffectQuality::Low => 0.0_f32,
            GraphicsEffectQuality::High => 1.0_f32,
        };
        self.evaluated_technique_pass_material_assets
            .iter()
            .reduce(|selected, candidate| {
                let selected_distance = selected
                    .quality_annotation
                    .map_or(f32::INFINITY, |quality| (quality - wanted_quality).abs());
                let candidate_distance = candidate
                    .quality_annotation
                    .map_or(f32::INFINITY, |quality| (quality - wanted_quality).abs());
                if candidate_distance < selected_distance {
                    candidate
                } else {
                    selected
                }
            })
            .map_or(&[], |technique| technique.pass_material_assets.as_ref())
    }

    pub(crate) fn first_bound_texture_asset(&self) -> Option<&Handle<Image>> {
        self.bound_texture_assets.first()
    }
}
