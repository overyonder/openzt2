//! Lowers authored PSYS emitter identity, intrinsic modifiers, shape, spawn policy, and capacity.

use openzt2_game_data::{
    particle::authored_particle_system::{
        AuthoredParticleCurvePoint as EffectCurvePoint,
        AuthoredParticleEmitterShape as EmitterShape, AuthoredParticleSpawnPolicy as SpawnPolicy,
        AuthoredParticleSystemModifier as EffectModifier,
    },
    AssetId,
};

use super::{
    particle_system_source_conversion_error::{
        particle_system_source_conversion_failure, ParticleSystemSourceConversionError,
    },
    particle_system_source_node_traversal::{
        describe_particle_system_source_node, particle_system_source_children_named,
    },
    particle_system_source_scalar_reading::{
        read_particle_system_attribute_bool_or_default,
        read_particle_system_parameter_f32_or_default,
        read_particle_system_parameter_u16_or_default, read_required_particle_system_attribute_f32,
        read_required_particle_system_attribute_u32,
    },
    particle_system_source_vector_reading::read_particle_system_vector3_parameter_or_default,
};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

pub(super) fn create_stable_particle_system_emitter_id(
    particle_system_asset_path: &str,
    simulator_handle: u32,
    emitter_handle: u32,
    emitter_ordinal: usize,
) -> AssetId {
    AssetId::from_key(&format!(
        "{particle_system_asset_path}.__emitter/psys_{simulator_handle:08x}_{emitter_handle:08x}_{emitter_ordinal:04x}"
    ))
}

pub(super) fn lower_particle_system_emitter_intrinsic_modifiers(
    emitter_node: &OrderedSourceDocumentNode,
) -> Result<Vec<EffectModifier>, ParticleSystemSourceConversionError> {
    let first_speed = read_particle_system_parameter_f32_or_default(emitter_node, "minspeed", 0.0)?;
    let second_speed =
        read_particle_system_parameter_f32_or_default(emitter_node, "maxspeed", first_speed)?;
    Ok(vec![EffectModifier::InitialVelocityCone {
        base: read_particle_system_vector3_parameter_or_default(
            emitter_node,
            "velocity base",
            [0.0; 3],
        )?,
        direction: read_particle_system_vector3_parameter_or_default(
            emitter_node,
            "initdir",
            [0.0, 0.0, 1.0],
        )?,
        vertical_direction: 0.0,
        vertical_angle: 0.0,
        horizontal_direction: 0.0,
        horizontal_angle: 0.0,
        min_speed: first_speed.min(second_speed),
        max_speed: first_speed.max(second_speed),
    }])
}

pub(super) fn lower_particle_system_emitter_shape_and_spawn_policy(
    particle_system_asset_path: &str,
    emitter_node: &OrderedSourceDocumentNode,
    effect_modifiers: &mut Vec<EffectModifier>,
) -> Result<(EmitterShape, SpawnPolicy), ParticleSystemSourceConversionError> {
    match read_required_particle_system_attribute_u32(emitter_node, "type")? {
        1 | 4 => Ok((
            lower_particle_system_cone_emitter_shape(emitter_node)?,
            lower_particle_system_cone_emitter_spawn_policy(emitter_node, effect_modifiers)?,
        )),
        2 => Ok((EmitterShape::Proxy, SpawnPolicy::Manual)),
        3 => Ok((EmitterShape::Decal, SpawnPolicy::Manual)),
        5 => {
            effect_modifiers.push(EffectModifier::TileRandomizer {
                columns: read_particle_system_parameter_u16_or_default(
                    emitter_node,
                    "number tiles u",
                    1,
                )?,
                rows: read_particle_system_parameter_u16_or_default(
                    emitter_node,
                    "number tiles v",
                    1,
                )?,
            });
            Ok((EmitterShape::Point, SpawnPolicy::Manual))
        }
        unknown_emitter_type => Err(particle_system_source_conversion_failure(
            particle_system_asset_path,
            describe_particle_system_source_node(emitter_node),
            format!("unknown emitter type {unknown_emitter_type}"),
        )),
    }
}

fn lower_particle_system_cone_emitter_shape(
    emitter_node: &OrderedSourceDocumentNode,
) -> Result<EmitterShape, ParticleSystemSourceConversionError> {
    let mut up_axis = read_particle_system_vector3_parameter_or_default(
        emitter_node,
        "areaout",
        [1.0, 0.0, 0.0],
    )?;
    let mut out_axis =
        read_particle_system_vector3_parameter_or_default(emitter_node, "areaup", [0.0, 1.0, 0.0])?;
    if up_axis == [0.0; 3] {
        up_axis = [1.0, 0.0, 0.0];
    }
    if out_axis == [0.0; 3] {
        out_axis = [0.0, 1.0, 0.0];
    }
    Ok(EmitterShape::Cone {
        origin: read_particle_system_vector3_parameter_or_default(
            emitter_node,
            "initpos",
            [0.0; 3],
        )?,
        direction: read_particle_system_vector3_parameter_or_default(
            emitter_node,
            "initdir",
            [0.0, 0.0, 1.0],
        )?,
        up: up_axis,
        out: out_axis,
        min_radius: read_particle_system_parameter_f32_or_default(emitter_node, "min radius", 0.0)?,
        max_radius: read_particle_system_parameter_f32_or_default(emitter_node, "max radius", 0.0)?,
    })
}

fn lower_particle_system_cone_emitter_spawn_policy(
    emitter_node: &OrderedSourceDocumentNode,
    effect_modifiers: &mut Vec<EffectModifier>,
) -> Result<SpawnPolicy, ParticleSystemSourceConversionError> {
    let spawn_probability =
        read_particle_system_parameter_f32_or_default(emitter_node, "probability", 1.0)?;
    if !(0.0..=1.0).contains(&spawn_probability) {
        return Err(particle_system_source_conversion_failure(
            "PSYS",
            describe_particle_system_source_node(emitter_node),
            "probability outside 0..1",
        ));
    }
    if let Some(birth_rate_node) =
        particle_system_source_children_named(emitter_node, "birthrate").next()
    {
        let playback_is_looped =
            read_particle_system_attribute_bool_or_default(birth_rate_node, "looped", false)?;
        let mut birth_rate_curve_points =
            particle_system_source_children_named(birth_rate_node, "key")
                .map(|key_node| {
                    Ok(EffectCurvePoint {
                        time: read_required_particle_system_attribute_f32(key_node, "time")?,
                        value: [
                            read_required_particle_system_attribute_f32(key_node, "value")?
                                * spawn_probability,
                            0.0,
                            0.0,
                            0.0,
                        ],
                    })
                })
                .collect::<Result<Vec<_>, ParticleSystemSourceConversionError>>()?;
        birth_rate_curve_points
            .sort_by(|left_point, right_point| left_point.time.total_cmp(&right_point.time));
        birth_rate_curve_points.dedup_by(|left_point, right_point| {
            if left_point.time == right_point.time {
                *left_point = *right_point;
                true
            } else {
                false
            }
        });
        if !birth_rate_curve_points.is_empty() {
            effect_modifiers.push(EffectModifier::BirthRateCurve {
                points: birth_rate_curve_points,
                looped: playback_is_looped,
            });
            return Ok(SpawnPolicy::Curve {
                looped: playback_is_looped,
            });
        }
    }
    Ok(SpawnPolicy::Rate {
        per_second: read_particle_system_parameter_f32_or_default(
            emitter_node,
            "ratetospawn",
            0.0,
        )? * spawn_probability,
    })
}

pub(super) fn calculate_particle_system_emitter_capacity(
    particle_system_asset_path: &str,
    simulator_node: &OrderedSourceDocumentNode,
    effect_modifiers: &[EffectModifier],
    spawn_policy: SpawnPolicy,
) -> Result<u32, ParticleSystemSourceConversionError> {
    let maximum_particle_lifetime_seconds = effect_modifiers
        .iter()
        .find_map(|effect_modifier| match effect_modifier {
            EffectModifier::InitialPosition { lifetime_seconds } => Some(*lifetime_seconds),
            EffectModifier::LifetimeRange { max_seconds, .. } => Some(*max_seconds),
            _ => None,
        })
        .ok_or_else(|| {
            particle_system_source_conversion_failure(
                particle_system_asset_path,
                describe_particle_system_source_node(simulator_node),
                "simulator has no lifetime modifier",
            )
        })?;
    let peak_birth_rate_per_second = effect_modifiers
        .iter()
        .find_map(|effect_modifier| match effect_modifier {
            EffectModifier::BirthRateCurve { points, .. } => Some(
                points
                    .iter()
                    .map(|curve_point| curve_point.value[0])
                    .fold(0.0, f32::max),
            ),
            _ => None,
        })
        .unwrap_or_else(|| match spawn_policy {
            SpawnPolicy::Rate { per_second } => per_second,
            _ => 1.0,
        });
    Ok(
        (peak_birth_rate_per_second.max(1.0) * maximum_particle_lifetime_seconds.max(1.0))
            .ceil()
            .clamp(1.0, 1_048_576.0) as u32,
    )
}
