use super::terrain_water_renderer_types::AuthoredTerrainWaterRendererTargets;
use super::terrain_water_renderer_types::WATER_RENDER_TARGET_SIDE;
use crate::assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use bevy::prelude::*;
use openzt2_game_data::terrain::TerrainBiomeWaterPresentation;

pub(super) fn bind_authored_water_surface_textures(
    pass: &mut EffectPassMaterial,
    targets: &AuthoredTerrainWaterRendererTargets,
    terrain_asset: &TerrainAsset,
    presentation: &TerrainBiomeWaterPresentation,
) -> bool {
    let bump_map_was_bound = pass
        .replace_texture_asset_for_effect_semantic("BumpMap", targets.combined_bump_map.clone());
    let refraction_was_bound =
        pass.replace_texture_asset_for_effect_semantic("EnvRefract", targets.refraction.clone());
    let reflection_was_bound =
        pass.replace_texture_asset_for_effect_semantic("EnvReflect", targets.reflection.clone());
    if !bump_map_was_bound || !(refraction_was_bound || reflection_was_bound) {
        return false;
    }
    // A declared texture need not be sampled by the selected technique.
    // The shipped FlatSpec passes use bump and environment maps only.
    if let Some(texture) = terrain_asset.texture_image(&presentation.gloss_map) {
        pass.replace_texture_asset_for_effect_semantic("ScumMap", texture.clone());
    }
    true
}

pub(super) fn bind_authored_water_material_values(
    pass: &mut EffectPassMaterial,
    presentation: &TerrainBiomeWaterPresentation,
    water_height: f32,
) {
    let reflection = presentation.surface_material.reflection;
    let refraction = presentation.surface_material.refraction;
    let rgb = |colour: [u8; 3]| Vec3::from_array(colour.map(|value| f32::from(value) / 255.0));
    let percent = |value: f32| value / 100.0;
    pass.bind_float_vector_effect_semantic(
        "ReflectBaseTint",
        rgb(reflection.ambient_colour_rgb255).extend(1.0),
    );
    pass.bind_float_vector_effect_semantic(
        "ReflectSpecTint",
        rgb(reflection.tint_colour_rgb255).extend(percent(reflection.strength_percent)),
    );
    pass.bind_float_vector_effect_semantic(
        "RefractBaseTint",
        rgb(refraction.ambient_colour_rgb255).extend(1.0),
    );
    pass.bind_float_vector_effect_semantic(
        "RefractSpecTint",
        rgb(refraction.tint_colour_rgb255).extend(1.0),
    );
    pass.bind_float_vector_effect_semantic(
        "ReflectSpecAtten",
        Vec4::new(
            -reflection.falloff_metres,
            -2.0 / reflection.falloff_metres.max(f32::EPSILON),
            percent(reflection.bumpiness_percent).clamp(0.0, 1.0) * 0.95 + 0.05,
            reflection.map_size_metres.clamp(1.0, 100.0).recip(),
        ),
    );
    pass.bind_float_vector_effect_semantic(
        "RefractSpecAtten",
        Vec4::new(
            -refraction.falloff_metres,
            -2.0 / refraction.falloff_metres.max(f32::EPSILON),
            percent(refraction.bumpiness_percent).clamp(0.0, 1.0) * 0.95 + 0.05,
            refraction.map_size_metres.clamp(1.0, 100.0).recip(),
        ),
    );
    pass.bind_float_vector_effect_semantic(
        "ReflectOpacity",
        above_water_reflection_opacity_coefficients(reflection.fresnel_percent),
    );
    pass.bind_float_vector_effect_semantic("RefractOpacity", Vec4::new(1.0, 1.0, 0.0, 0.0));
    let reciprocal_target_side = (WATER_RENDER_TARGET_SIDE as f32).recip();
    pass.bind_float_vector_effect_semantic("ReflectSize", Vec4::splat(reciprocal_target_side));
    pass.bind_float_vector_effect_semantic("RefractSize", Vec4::splat(reciprocal_target_side));
    pass.bind_float_vector_effect_semantic("DepthOffset", Vec4::new(0.0, 0.0, 0.0, water_height));
}

fn above_water_reflection_opacity_coefficients(fresnel_percent: f32) -> Vec4 {
    // Native water initializes the angular intercept to one and updates the
    // slope from Fresnel. Strength scales ReflectSpecTint, not this slope.
    // The shader evaluates saturate(intercept + abs(normal.dot(eye)) * slope).
    Vec4::new(1.0, 0.5 - 1.5 * (fresnel_percent / 100.0), 0.0, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_fresnel_reduces_reflection_when_viewing_from_above() {
        for (fresnel, expected_slope) in [(0.0, 0.5), (50.0, -0.25), (100.0, -1.0)] {
            let coefficients = above_water_reflection_opacity_coefficients(fresnel);
            assert_eq!(coefficients, Vec4::new(1.0, expected_slope, 0.0, 0.0));
        }
        let coefficients = above_water_reflection_opacity_coefficients(100.0);
        let grazing_attenuation = coefficients.x.clamp(0.0, 1.0);
        let overhead_attenuation = (coefficients.x + coefficients.y).clamp(0.0, 1.0);
        assert_eq!(grazing_attenuation.to_bits(), 1.0_f32.to_bits());
        assert_eq!(overhead_attenuation.to_bits(), 0.0_f32.to_bits());
    }
}
