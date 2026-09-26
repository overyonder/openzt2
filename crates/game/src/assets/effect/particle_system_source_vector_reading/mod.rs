//! Converts repaired PSYS parameter arrays to fixed-size vectors.

use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

use super::{
    particle_system_source_conversion_error::{
        particle_system_source_conversion_failure, ParticleSystemSourceConversionError,
    },
    particle_system_source_node_traversal::{
        describe_particle_system_source_node, find_particle_system_parameter,
    },
    particle_system_source_scalar_reading::parse_particle_system_parameter_float_values,
};

pub(super) fn convert_particle_system_values_to_vector3(
    source_values: Vec<f32>,
    particle_system_asset_path: &str,
    source_node: &OrderedSourceDocumentNode,
    parameter_label: &str,
) -> Result<[f32; 3], ParticleSystemSourceConversionError> {
    source_values.try_into().map_err(|_| {
        particle_system_source_conversion_failure(
            particle_system_asset_path,
            describe_particle_system_source_node(source_node),
            format!("{parameter_label} does not contain three values"),
        )
    })
}

pub(super) fn convert_particle_system_values_to_vector4(
    source_values: Vec<f32>,
    particle_system_asset_path: &str,
    source_node: &OrderedSourceDocumentNode,
    parameter_label: &str,
) -> Result<[f32; 4], ParticleSystemSourceConversionError> {
    source_values.try_into().map_err(|_| {
        particle_system_source_conversion_failure(
            particle_system_asset_path,
            describe_particle_system_source_node(source_node),
            format!("{parameter_label} does not contain four values"),
        )
    })
}

pub(super) fn read_particle_system_vector3_parameter_or_default(
    source_node: &OrderedSourceDocumentNode,
    parameter_label: &str,
    default_value: [f32; 3],
) -> Result<[f32; 3], ParticleSystemSourceConversionError> {
    find_particle_system_parameter(source_node, parameter_label)
        .map(parse_particle_system_parameter_float_values)
        .transpose()?
        .map(|source_values| {
            convert_particle_system_values_to_vector3(
                source_values,
                "PSYS",
                source_node,
                parameter_label,
            )
        })
        .transpose()
        .map(|parameter_value| parameter_value.unwrap_or(default_value))
}
