//! Curve construction from authored PSYS age and value arrays.

use openzt2_game_data::particle::authored_particle_system::AuthoredParticleCurvePoint as EffectCurvePoint;

use super::{
    particle_system_source_conversion_error::{
        particle_system_source_conversion_failure, ParticleSystemSourceConversionError,
    },
    particle_system_source_node_traversal::describe_particle_system_source_node,
};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

pub(super) fn lower_particle_system_color_with_four_alpha_keys(
    base_color: [f32; 4],
    alpha_values: &[f32],
    interior_key_ages: &[f32],
    particle_system_asset_path: &str,
    source_node: &OrderedSourceDocumentNode,
) -> Result<Vec<EffectCurvePoint>, ParticleSystemSourceConversionError> {
    if alpha_values.len() != 4 || interior_key_ages.len() != 2 {
        return Err(particle_system_source_conversion_failure(
            particle_system_asset_path,
            describe_particle_system_source_node(source_node),
            "Fade requires four alpha and two age values",
        ));
    }
    Ok(normalize_particle_system_curve_points(
        [0.0, interior_key_ages[0], interior_key_ages[1], 1.0]
            .into_iter()
            .zip(alpha_values)
            .map(|(key_age, alpha_value)| EffectCurvePoint {
                time: key_age,
                value: [base_color[0], base_color[1], base_color[2], *alpha_value],
            })
            .collect(),
    ))
}

pub(super) fn lower_particle_system_four_key_scalar_curve(
    scalar_values: &[f32],
    interior_key_ages: &[f32],
    particle_system_asset_path: &str,
    source_node: &OrderedSourceDocumentNode,
) -> Result<Vec<EffectCurvePoint>, ParticleSystemSourceConversionError> {
    if scalar_values.len() != 4 || interior_key_ages.len() != 2 {
        return Err(particle_system_source_conversion_failure(
            particle_system_asset_path,
            describe_particle_system_source_node(source_node),
            "Size requires four values and two ages",
        ));
    }
    Ok(normalize_particle_system_curve_points(
        [0.0, interior_key_ages[0], interior_key_ages[1], 1.0]
            .into_iter()
            .zip(scalar_values)
            .map(|(key_age, scalar_value)| EffectCurvePoint {
                time: key_age,
                value: [*scalar_value; 4],
            })
            .collect(),
    ))
}

pub(super) fn lower_particle_system_four_key_rgba_curve(
    rgba_values: &[f32],
    interior_key_ages: &[f32],
    particle_system_asset_path: &str,
    source_node: &OrderedSourceDocumentNode,
) -> Result<Vec<EffectCurvePoint>, ParticleSystemSourceConversionError> {
    if rgba_values.len() != 16 || interior_key_ages.len() != 2 {
        return Err(particle_system_source_conversion_failure(
            particle_system_asset_path,
            describe_particle_system_source_node(source_node),
            "Color requires four RGBA values and two ages",
        ));
    }
    Ok(normalize_particle_system_curve_points(
        [0.0, interior_key_ages[0], interior_key_ages[1], 1.0]
            .into_iter()
            .enumerate()
            .map(|(key_index, key_age)| EffectCurvePoint {
                time: key_age,
                value: rgba_values[key_index * 4..key_index * 4 + 4]
                    .try_into()
                    .unwrap_or([0.0; 4]),
            })
            .collect(),
    ))
}

fn normalize_particle_system_curve_points(
    mut curve_points: Vec<EffectCurvePoint>,
) -> Vec<EffectCurvePoint> {
    curve_points
        .iter_mut()
        .for_each(|curve_point| curve_point.time = curve_point.time.clamp(0.0, 1.0));
    curve_points.sort_by(|left_point, right_point| left_point.time.total_cmp(&right_point.time));
    curve_points.dedup_by(|left_point, right_point| left_point.time == right_point.time);
    curve_points
}
