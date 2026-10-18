use bevy::prelude::*;

use super::{
    material_asset_types::MaterialAsset, runtime::effect_pass_gpu_data::EffectPassMaterial,
};
use crate::graphics_settings_types::GraphicsEffectQuality;

impl MaterialAsset {
    pub fn evaluated_d3d9_effect(&self) -> &d3d9_effects::effect_types::EvaluatedD3d9Effect {
        &self.evaluated_d3d9_effect
    }

    pub fn evaluated_pass_material_assets_for_effect_quality(
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

    pub fn first_bound_texture_asset(&self) -> Option<&Handle<Image>> {
        self.bound_texture_assets
            .first()
            .map(|binding| &binding.texture)
    }

    /// Resolves an authored texture parameter by its name or effect semantic.
    pub fn bound_texture_asset(&self, name: &str) -> Option<&Handle<Image>> {
        let parameter = self
            .evaluated_d3d9_effect
            .parameter_descriptions
            .iter()
            .find(|parameter| {
                parameter.parameter_name.eq_ignore_ascii_case(name)
                    || parameter
                        .semantic_name
                        .as_deref()
                        .is_some_and(|semantic| semantic.eq_ignore_ascii_case(name))
            });
        self.bound_texture_assets
            .iter()
            .rev()
            .find(|binding| {
                binding.parameter_name.eq_ignore_ascii_case(name)
                    || parameter.is_some_and(|parameter| {
                        binding
                            .parameter_name
                            .eq_ignore_ascii_case(&parameter.parameter_name)
                    })
            })
            .map(|binding| &binding.texture)
    }
}
