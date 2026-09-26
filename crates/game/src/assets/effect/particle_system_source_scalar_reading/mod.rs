//! Reads and validates scalar parameter and attribute values from repaired PSYS nodes.

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

use super::{
    particle_system_source_conversion_error::{
        particle_system_source_conversion_failure, ParticleSystemSourceConversionError,
    },
    particle_system_source_node_traversal::{
        describe_particle_system_source_node, find_particle_system_parameter,
        find_particle_system_parameter_by_any_label,
    },
};

pub(super) fn parse_particle_system_parameter_float_values(
    parameter_node: &OrderedSourceDocumentNode,
) -> Result<Vec<f32>, ParticleSystemSourceConversionError> {
    parameter_node
        .first_text()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|source_value| !source_value.is_empty())
        .map(|source_value| {
            source_value.parse().map_err(|_| {
                particle_system_source_conversion_failure(
                    "PSYS",
                    describe_particle_system_source_node(parameter_node),
                    format!("invalid number {source_value:?}"),
                )
            })
        })
        .collect()
}

pub(super) fn read_required_particle_system_parameter_float_values(
    source_node: &OrderedSourceDocumentNode,
    parameter_label: &str,
) -> Result<Vec<f32>, ParticleSystemSourceConversionError> {
    find_particle_system_parameter(source_node, parameter_label)
        .ok_or_else(|| {
            particle_system_source_conversion_failure(
                "PSYS",
                describe_particle_system_source_node(source_node),
                format!("missing parameter {parameter_label}"),
            )
        })
        .and_then(parse_particle_system_parameter_float_values)
}

pub(super) fn read_required_particle_system_parameter_float_values_by_any_label(
    source_node: &OrderedSourceDocumentNode,
    accepted_parameter_labels: &[&str],
) -> Result<Vec<f32>, ParticleSystemSourceConversionError> {
    find_particle_system_parameter_by_any_label(source_node, accepted_parameter_labels)
        .ok_or_else(|| {
            particle_system_source_conversion_failure(
                "PSYS",
                describe_particle_system_source_node(source_node),
                format!("missing parameter {}", accepted_parameter_labels.join("/"),),
            )
        })
        .and_then(parse_particle_system_parameter_float_values)
}

pub(super) fn read_required_particle_system_parameter_f32(
    source_node: &OrderedSourceDocumentNode,
    parameter_label: &str,
) -> Result<f32, ParticleSystemSourceConversionError> {
    read_optional_particle_system_parameter_f32(source_node, parameter_label)?.ok_or_else(|| {
        particle_system_source_conversion_failure(
            "PSYS",
            describe_particle_system_source_node(source_node),
            format!("missing parameter {parameter_label}"),
        )
    })
}

pub(super) fn read_required_particle_system_parameter_f32_by_any_label(
    source_node: &OrderedSourceDocumentNode,
    accepted_parameter_labels: &[&str],
) -> Result<f32, ParticleSystemSourceConversionError> {
    read_required_particle_system_parameter_float_values_by_any_label(
        source_node,
        accepted_parameter_labels,
    )?
    .first()
    .copied()
    .ok_or_else(|| {
        particle_system_source_conversion_failure(
            "PSYS",
            describe_particle_system_source_node(source_node),
            "empty scalar",
        )
    })
}

pub(super) fn read_optional_particle_system_parameter_f32(
    source_node: &OrderedSourceDocumentNode,
    parameter_label: &str,
) -> Result<Option<f32>, ParticleSystemSourceConversionError> {
    find_particle_system_parameter(source_node, parameter_label)
        .map(parse_particle_system_parameter_float_values)
        .transpose()
        .map(|parameter_values| {
            parameter_values.and_then(|parameter_values| parameter_values.first().copied())
        })
}

pub(super) fn read_particle_system_parameter_f32_or_default(
    source_node: &OrderedSourceDocumentNode,
    parameter_label: &str,
    default_value: f32,
) -> Result<f32, ParticleSystemSourceConversionError> {
    Ok(
        read_optional_particle_system_parameter_f32(source_node, parameter_label)?
            .unwrap_or(default_value),
    )
}

pub(super) fn read_required_particle_system_parameter_u32(
    source_node: &OrderedSourceDocumentNode,
    parameter_label: &str,
) -> Result<u32, ParticleSystemSourceConversionError> {
    convert_particle_system_parameter_f32_to_u32(
        read_required_particle_system_parameter_f32(source_node, parameter_label)?,
        source_node,
        parameter_label,
    )
}

pub(super) fn read_required_particle_system_parameter_u16(
    source_node: &OrderedSourceDocumentNode,
    parameter_label: &str,
) -> Result<u16, ParticleSystemSourceConversionError> {
    u16::try_from(read_required_particle_system_parameter_u32(
        source_node,
        parameter_label,
    )?)
    .map_err(|_| {
        particle_system_source_conversion_failure(
            "PSYS",
            describe_particle_system_source_node(source_node),
            format!("{parameter_label} exceeds u16"),
        )
    })
}

pub(super) fn read_particle_system_parameter_u16_or_default(
    source_node: &OrderedSourceDocumentNode,
    parameter_label: &str,
    default_value: u16,
) -> Result<u16, ParticleSystemSourceConversionError> {
    read_optional_particle_system_parameter_f32(source_node, parameter_label)?
        .map(|parameter_value| {
            convert_particle_system_parameter_f32_to_u32(
                parameter_value,
                source_node,
                parameter_label,
            )
            .and_then(|integer_value| {
                u16::try_from(integer_value).map_err(|_| {
                    particle_system_source_conversion_failure(
                        "PSYS",
                        describe_particle_system_source_node(source_node),
                        format!("{parameter_label} exceeds u16"),
                    )
                })
            })
        })
        .transpose()
        .map(|parameter_value| parameter_value.unwrap_or(default_value))
}

pub(super) fn read_particle_system_parameter_bool_or_default(
    source_node: &OrderedSourceDocumentNode,
    parameter_label: &str,
    default_value: bool,
) -> Result<bool, ParticleSystemSourceConversionError> {
    find_particle_system_parameter(source_node, parameter_label)
        .and_then(OrderedSourceDocumentNode::first_text)
        .map(|source_value| parse_particle_system_bool(source_value, source_node))
        .transpose()
        .map(|parameter_value| parameter_value.unwrap_or(default_value))
}

fn convert_particle_system_parameter_f32_to_u32(
    parameter_value: f32,
    source_node: &OrderedSourceDocumentNode,
    parameter_label: &str,
) -> Result<u32, ParticleSystemSourceConversionError> {
    if parameter_value.is_finite()
        && parameter_value >= 0.0
        && parameter_value.fract() == 0.0
        && parameter_value <= u32::MAX as f32
    {
        Ok(parameter_value as u32)
    } else {
        Err(particle_system_source_conversion_failure(
            "PSYS",
            describe_particle_system_source_node(source_node),
            format!("{parameter_label} is not a non-negative integer"),
        ))
    }
}

pub(super) fn read_required_particle_system_attribute_u32(
    source_node: &OrderedSourceDocumentNode,
    attribute_name: &str,
) -> Result<u32, ParticleSystemSourceConversionError> {
    source_node
        .attribute(attribute_name)
        .ok_or_else(|| {
            particle_system_source_conversion_failure(
                "PSYS",
                describe_particle_system_source_node(source_node),
                format!("missing attribute {attribute_name}"),
            )
        })?
        .parse()
        .map_err(|_| {
            particle_system_source_conversion_failure(
                "PSYS",
                describe_particle_system_source_node(source_node),
                format!("invalid integer attribute {attribute_name}"),
            )
        })
}

pub(super) fn read_required_particle_system_attribute_f32(
    source_node: &OrderedSourceDocumentNode,
    attribute_name: &str,
) -> Result<f32, ParticleSystemSourceConversionError> {
    source_node
        .attribute(attribute_name)
        .ok_or_else(|| {
            particle_system_source_conversion_failure(
                "PSYS",
                describe_particle_system_source_node(source_node),
                format!("missing attribute {attribute_name}"),
            )
        })?
        .parse()
        .map_err(|_| {
            particle_system_source_conversion_failure(
                "PSYS",
                describe_particle_system_source_node(source_node),
                format!("invalid float attribute {attribute_name}"),
            )
        })
}

pub(super) fn read_particle_system_attribute_bool_or_default(
    source_node: &OrderedSourceDocumentNode,
    attribute_name: &str,
    default_value: bool,
) -> Result<bool, ParticleSystemSourceConversionError> {
    source_node
        .attribute(attribute_name)
        .map(|source_value| parse_particle_system_bool(source_value, source_node))
        .transpose()
        .map(|attribute_value| attribute_value.unwrap_or(default_value))
}

fn parse_particle_system_bool(
    source_value: &str,
    source_node: &OrderedSourceDocumentNode,
) -> Result<bool, ParticleSystemSourceConversionError> {
    match source_value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(particle_system_source_conversion_failure(
            "PSYS",
            describe_particle_system_source_node(source_node),
            format!("invalid boolean {source_value:?}"),
        )),
    }
}
