//! Mapping from translated D3D9 shader inputs to Bevy mesh attributes.

use bevy::prelude::*;

pub(super) fn map_d3d9_shader_input_to_bevy_vertex_attribute(
    input: &d3d9_effects::shader_types::D3d9ShaderInputSignatureElement,
) -> Option<(bevy::mesh::MeshVertexAttribute, u32)> {
    let semantic = input.semantic_name.to_ascii_uppercase();
    let attribute = match (semantic.as_str(), input.semantic_index) {
        ("POSITION", 0) => Mesh::ATTRIBUTE_POSITION,
        ("NORMAL", 0) => Mesh::ATTRIBUTE_NORMAL,
        ("TANGENT", 0) => Mesh::ATTRIBUTE_TANGENT,
        ("COLOR", 0) => Mesh::ATTRIBUTE_COLOR,
        ("TEXCOORD", 0) => Mesh::ATTRIBUTE_UV_0,
        ("TEXCOORD", 1) => Mesh::ATTRIBUTE_UV_1,
        ("BLENDINDICES", 0) => Mesh::ATTRIBUTE_JOINT_INDEX,
        ("BLENDWEIGHT", 0) => Mesh::ATTRIBUTE_JOINT_WEIGHT,
        _ => return None,
    };
    Some((attribute, input.input_register))
}
