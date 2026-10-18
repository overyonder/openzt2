use bevy::prelude::*;
use openzt2_game_assets::material::runtime::effect_pass_gpu_data::EffectPassMaterial;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::terrain::TerrainBiomeWaterPresentation;
use openzt2_zoo_state::terrain::terrain_water_renderer_types::AuthoredTerrainWaterRendererTargets;

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
    // A declared texture need not be sampled by the selected technique.
    // The shipped FlatSpec passes use bump and environment maps only.
    let scum_map = presentation
        .surface_material
        .configuration
        .scum
        .as_ref()
        .and_then(|scum| scum.image.as_deref())
        .and_then(|image| terrain_asset.texture_image(image));
    let scum_was_bound = scum_map.is_some_and(|texture| {
        pass.replace_texture_asset_for_effect_semantic("ScumMap", texture.clone())
    });
    (bump_map_was_bound && (refraction_was_bound || reflection_was_bound)) || scum_was_bound
}

pub(super) fn bind_authored_water_material_values(
    pass: &mut EffectPassMaterial,
    presentation: &TerrainBiomeWaterPresentation,
    water_height: f32,
) {
    let reflection = presentation.surface_material.reflection;
    let refraction = presentation.surface_material.refraction;
    let reflection_falloff = reflection.falloff_metres.clamp(1.0, 100_000.0);
    let refraction_falloff = refraction.falloff_metres.clamp(1.0, 100_000.0);
    let rgb = |colour: [u8; 3]| Vec3::from_array(colour.map(|value| f32::from(value) / 255.0));
    let percent = |value: f32| value / 100.0;
    pass.bind_float_vector_effect_semantic(
        "ReflectBaseTint",
        rgb(reflection.ambient_colour_rgb255).extend(1.0),
    );
    pass.bind_float_vector_effect_semantic(
        "ReflectSpecTint",
        rgb(reflection.tint_colour_rgb255)
            .extend(percent(reflection.strength_percent).clamp(0.0, 1.0)),
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
            -reflection_falloff,
            -2.0 / reflection_falloff,
            percent(reflection.bumpiness_percent).clamp(0.0, 1.0) * 0.95 + 0.05,
            reflection.map_size_metres.clamp(1.0, 100.0).recip(),
        ),
    );
    pass.bind_float_vector_effect_semantic(
        "RefractSpecAtten",
        Vec4::new(
            -refraction_falloff,
            -2.0 / refraction_falloff,
            percent(refraction.bumpiness_percent).clamp(0.0, 1.0) * 0.95 + 0.05,
            refraction.map_size_metres.clamp(1.0, 100.0).recip(),
        ),
    );
    pass.bind_float_vector_effect_semantic(
        "ReflectOpacity",
        above_water_reflection_opacity_coefficients(reflection.fresnel_percent),
    );
    pass.bind_float_vector_effect_semantic("RefractOpacity", Vec4::new(1.0, 1.0, 0.0, 0.0));
    pass.bind_float_vector_effect_semantic("DepthOffset", Vec4::new(0.0, 0.0, 0.0, water_height));
}

fn above_water_reflection_opacity_coefficients(fresnel_percent: f32) -> Vec4 {
    // The effect subtracts x from y when computing angular attenuation.
    // Strength scales ReflectSpecTint independently of these endpoints.
    Vec4::new(1.0, 0.5 - 1.5 * (fresnel_percent / 100.0), 0.0, 0.05)
}

pub(super) fn bind_authored_water_frame_base_tints(
    pass: &mut EffectPassMaterial,
    presentation: &TerrainBiomeWaterPresentation,
    ambient_colour: Vec3,
    underwater: bool,
    capture_fraction: Option<f32>,
    below_plane_capture_fraction: Option<f32>,
    training_opacity_mix: f32,
) {
    let rgb = |colour: [u8; 3]| {
        Vec3::from_array(colour.map(|value| f32::from(value) / 255.0)) * ambient_colour
    };
    let refraction = rgb(presentation
        .surface_material
        .refraction
        .ambient_colour_rgb255);
    let reflection = rgb(presentation
        .surface_material
        .reflection
        .ambient_colour_rgb255);
    let alpha = capture_fraction.unwrap_or(1.0);
    pass.bind_float_vector_effect_semantic("RefractBaseTint", refraction.extend(alpha));
    pass.bind_float_vector_effect_semantic(
        "ReflectBaseTint",
        if underwater {
            reflection.max(refraction)
        } else {
            reflection
        }
        .extend(alpha),
    );
    let mut opacity = above_water_reflection_opacity_coefficients(
        presentation.surface_material.reflection.fresnel_percent,
    );
    if let Some(fraction) = below_plane_capture_fraction {
        opacity.y = if fraction >= 1.0 {
            1.0
        } else {
            (1.0 - opacity.y) * fraction + opacity.y
        };
    }
    opacity.x += (1.0 - opacity.x) * training_opacity_mix;
    opacity.y += (-1.0 - opacity.y) * training_opacity_mix;
    pass.bind_float_vector_effect_semantic("ReflectOpacity", opacity);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_fresnel_reduces_reflection_when_viewing_from_above() {
        for (fresnel, expected_slope) in [(0.0, 0.5), (50.0, -0.25), (100.0, -1.0)] {
            let coefficients = above_water_reflection_opacity_coefficients(fresnel);
            assert_eq!(coefficients, Vec4::new(1.0, expected_slope, 0.0, 0.05));
        }
        let coefficients = above_water_reflection_opacity_coefficients(100.0);
        let grazing_attenuation = coefficients.x.clamp(0.0, 1.0);
        let overhead_attenuation =
            (coefficients.x + coefficients.y - coefficients.x).clamp(0.0, 1.0);
        assert_eq!(grazing_attenuation.to_bits(), 1.0_f32.to_bits());
        assert_eq!(overhead_attenuation.to_bits(), 0.0_f32.to_bits());
    }
}
