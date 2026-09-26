//! Lowers authored PSYS renderer nodes and resolves their material asset paths.

use openzt2_game_data::particle::authored_particle_system::{
    AuthoredParticleRendererKind as ParticleRendererKind,
    AuthoredParticleSystemModifier as EffectModifier,
};

use super::{
    particle_system_source_conversion_error::{
        particle_system_source_conversion_failure, ParticleSystemSourceConversionError,
    },
    particle_system_source_node_traversal::{
        describe_particle_system_source_node, find_particle_system_parameter,
    },
    particle_system_source_scalar_reading::{
        read_particle_system_parameter_bool_or_default,
        read_particle_system_parameter_f32_or_default,
        read_particle_system_parameter_u16_or_default, read_required_particle_system_attribute_u32,
    },
};
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

pub(super) fn lower_particle_system_renderer_node(
    particle_system_asset_path: &str,
    renderer_node: &OrderedSourceDocumentNode,
) -> Result<(String, EffectModifier), ParticleSystemSourceConversionError> {
    let renderer_kind = match read_required_particle_system_attribute_u32(renderer_node, "type")? {
        2 => ParticleRendererKind::Billboard,
        3 => ParticleRendererKind::VelocityAligned,
        5 => ParticleRendererKind::Decal,
        unknown_renderer_type => {
            return Err(particle_system_source_conversion_failure(
                particle_system_asset_path,
                describe_particle_system_source_node(renderer_node),
                format!("unknown renderer type {unknown_renderer_type}"),
            ));
        }
    };
    let authored_material_reference = find_particle_system_parameter(renderer_node, "material")
        .and_then(OrderedSourceDocumentNode::first_text)
        .map(str::trim)
        .ok_or_else(|| {
            particle_system_source_conversion_failure(
                particle_system_asset_path,
                describe_particle_system_source_node(renderer_node),
                "missing renderer material",
            )
        })?
        .replace('\\', "/");
    let material_asset_path =
        resolve_particle_system_material_asset_path(&authored_material_reference);
    Ok((
        material_asset_path,
        EffectModifier::Renderer {
            kind: renderer_kind,
            width: read_particle_system_parameter_f32_or_default(renderer_node, "width", 1.0)?,
            height: read_particle_system_parameter_f32_or_default(renderer_node, "height", 1.0)?,
            columns: read_particle_system_parameter_u16_or_default(renderer_node, "numtilesu", 1)?,
            rows: read_particle_system_parameter_u16_or_default(renderer_node, "numtilesv", 1)?,
            sort_by_distance: read_particle_system_parameter_bool_or_default(
                renderer_node,
                "sortbydistance",
                false,
            )?,
            minimum_height: read_particle_system_parameter_f32_or_default(
                renderer_node,
                "minheight",
                0.0,
            )?,
        },
    ))
}

fn resolve_particle_system_material_asset_path(authored_material_reference: &str) -> String {
    let case_folded_material_reference = authored_material_reference.to_ascii_lowercase();
    case_folded_material_reference
        .ends_with(".bfmat")
        .then_some(case_folded_material_reference.clone())
        .unwrap_or_else(|| format!("{case_folded_material_reference}.bfmat"))
}
