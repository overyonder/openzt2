//! Converts PSYS modifier nodes to effect modifiers.

use openzt2_game_data::{
    particle::authored_particle_system::AuthoredParticleSystemModifier as EffectModifier, AssetId,
};

use super::{
    particle_system_source_conversion_error::{
        particle_system_source_conversion_failure, ParticleSystemSourceConversionError,
    },
    particle_system_source_curve_lowering::{
        lower_particle_system_color_with_four_alpha_keys,
        lower_particle_system_four_key_rgba_curve, lower_particle_system_four_key_scalar_curve,
    },
    particle_system_source_node_traversal::describe_particle_system_source_node,
    particle_system_source_scalar_reading::{
        read_optional_particle_system_parameter_f32, read_required_particle_system_attribute_u32,
        read_required_particle_system_parameter_f32,
        read_required_particle_system_parameter_f32_by_any_label,
        read_required_particle_system_parameter_float_values,
        read_required_particle_system_parameter_float_values_by_any_label,
        read_required_particle_system_parameter_u16, read_required_particle_system_parameter_u32,
    },
    particle_system_source_vector_reading::{
        convert_particle_system_values_to_vector3, convert_particle_system_values_to_vector4,
    },
};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

pub(super) fn lower_particle_system_modifier_node(
    particle_system_asset_path: &str,
    modifier_node: &OrderedSourceDocumentNode,
    resolve_emitter_handle: &impl Fn(u32) -> Option<AssetId>,
) -> Result<EffectModifier, ParticleSystemSourceConversionError> {
    Ok(
        match read_required_particle_system_attribute_u32(modifier_node, "type")? {
            1 | 15 => {
                let first_lifetime_seconds =
                    read_required_particle_system_parameter_f32(modifier_node, "minlife")?;
                let second_lifetime_seconds =
                    read_required_particle_system_parameter_f32(modifier_node, "maxlife")?;
                EffectModifier::LifetimeRange {
                    min_seconds: first_lifetime_seconds.min(second_lifetime_seconds),
                    max_seconds: first_lifetime_seconds.max(second_lifetime_seconds),
                }
            }
            2 => EffectModifier::Drift { velocity: [0.0; 3] },
            3 => EffectModifier::DirectionalGravity {
                direction: [0.0, 0.0, 1.0],
                acceleration: read_required_particle_system_parameter_f32(modifier_node, "accel")?,
            },
            4 => EffectModifier::Drag {
                coefficient: read_required_particle_system_parameter_f32(modifier_node, "drag")?,
            },
            5 => EffectModifier::ColorOverLifetime {
                points: lower_particle_system_color_with_four_alpha_keys(
                    convert_particle_system_values_to_vector4(
                        read_required_particle_system_parameter_float_values(
                            modifier_node,
                            "initcolor",
                        )?,
                        particle_system_asset_path,
                        modifier_node,
                        "initcolor",
                    )?,
                    &read_required_particle_system_parameter_float_values(modifier_node, "fade")?,
                    &read_required_particle_system_parameter_float_values(modifier_node, "age")?,
                    particle_system_asset_path,
                    modifier_node,
                )?,
            },
            6 => EffectModifier::SizeOverLifetime {
                points: lower_particle_system_four_key_scalar_curve(
                    &read_required_particle_system_parameter_float_values(modifier_node, "value")?,
                    &read_required_particle_system_parameter_float_values(modifier_node, "age")?,
                    particle_system_asset_path,
                    modifier_node,
                )?,
            },
            7 => EffectModifier::InitialSizeRange {
                min: read_required_particle_system_parameter_f32(modifier_node, "minsize")?,
                max: read_required_particle_system_parameter_f32(modifier_node, "maxsize")?,
            },
            8 => {
                let maximum_spin_rate =
                    read_required_particle_system_parameter_f32(modifier_node, "maxrate")?;
                EffectModifier::Spin {
                    min_rate: read_optional_particle_system_parameter_f32(
                        modifier_node,
                        "minrate",
                    )?
                    .unwrap_or(-maximum_spin_rate.abs()),
                    max_rate: maximum_spin_rate,
                }
            }
            9 => EffectModifier::AngularDrag {
                coefficient: read_required_particle_system_parameter_f32(modifier_node, "drag")?,
            },
            10 => EffectModifier::TileRandomizer {
                columns: read_required_particle_system_parameter_u16(modifier_node, "numtilesu")?,
                rows: read_required_particle_system_parameter_u16(modifier_node, "numtilesv")?,
            },
            12 => EffectModifier::KillPlane {
                normal: convert_particle_system_values_to_vector3(
                    read_required_particle_system_parameter_float_values(
                        modifier_node,
                        "planenorm",
                    )?,
                    particle_system_asset_path,
                    modifier_node,
                    "planenorm",
                )?,
                distance: read_required_particle_system_parameter_f32(modifier_node, "planedist")?,
            },
            13 => EffectModifier::SpawnOnPlane {
                normal: convert_particle_system_values_to_vector3(
                    read_required_particle_system_parameter_float_values(
                        modifier_node,
                        "planenorm",
                    )?,
                    particle_system_asset_path,
                    modifier_node,
                    "planenorm",
                )?,
                distance: read_required_particle_system_parameter_f32(modifier_node, "planedist")?,
                probability: read_required_particle_system_parameter_f32(
                    modifier_node,
                    "probability",
                )?,
                min_count: read_required_particle_system_parameter_u32(
                    modifier_node,
                    "mintospawn",
                )?,
                max_count: read_required_particle_system_parameter_u32(
                    modifier_node,
                    "maxtospawn",
                )?,
                target_emitter: resolve_emitter_handle(
                    read_required_particle_system_parameter_u32(modifier_node, "proxy")?,
                )
                .ok_or_else(|| {
                    particle_system_source_conversion_failure(
                        particle_system_asset_path,
                        describe_particle_system_source_node(modifier_node),
                        "collision proxy is ambiguous",
                    )
                })?,
                radius: read_required_particle_system_parameter_f32(modifier_node, "radius")?,
                speed: read_required_particle_system_parameter_f32(modifier_node, "speed")?,
                velocity_out: read_required_particle_system_vector3(
                    particle_system_asset_path,
                    modifier_node,
                    "velout",
                )?,
                velocity_up: read_required_particle_system_vector3(
                    particle_system_asset_path,
                    modifier_node,
                    "velup",
                )?,
                velocity_direction: read_required_particle_system_vector3(
                    particle_system_asset_path,
                    modifier_node,
                    "veldir",
                )?,
                velocity_scale: read_required_particle_system_vector3(
                    particle_system_asset_path,
                    modifier_node,
                    "velscale",
                )?,
                offset: read_optional_particle_system_parameter_f32(modifier_node, "offset")?
                    .unwrap_or(0.0),
            },
            14 => EffectModifier::ColorOverLifetime {
                points: lower_particle_system_four_key_rgba_curve(
                    &read_required_particle_system_parameter_float_values_by_any_label(
                        modifier_node,
                        &["color", "Color"],
                    )?,
                    &read_required_particle_system_parameter_float_values_by_any_label(
                        modifier_node,
                        &["age", "Age"],
                    )?,
                    particle_system_asset_path,
                    modifier_node,
                )?,
            },
            16 => EffectModifier::DecalUv {
                minimum: read_required_particle_system_parameter_f32_by_any_label(
                    modifier_node,
                    &["uv min", "UV Min"],
                )?,
            },
            17 => EffectModifier::FlutterVelocity {
                acceleration: read_required_particle_system_parameter_f32(modifier_node, "accel")?,
            },
            18 => EffectModifier::FlutterPosition {
                acceleration: read_required_particle_system_parameter_f32(modifier_node, "accel")?,
            },
            19 => EffectModifier::Drift {
                velocity: read_required_particle_system_vector3(
                    particle_system_asset_path,
                    modifier_node,
                    "velocity",
                )?,
            },
            unknown_modifier_type => {
                return Err(particle_system_source_conversion_failure(
                    particle_system_asset_path,
                    describe_particle_system_source_node(modifier_node),
                    format!("unknown modifier type {unknown_modifier_type}"),
                ));
            }
        },
    )
}

fn read_required_particle_system_vector3(
    particle_system_asset_path: &str,
    source_node: &OrderedSourceDocumentNode,
    parameter_label: &str,
) -> Result<[f32; 3], ParticleSystemSourceConversionError> {
    convert_particle_system_values_to_vector3(
        read_required_particle_system_parameter_float_values(source_node, parameter_label)?,
        particle_system_asset_path,
        source_node,
        parameter_label,
    )
}
