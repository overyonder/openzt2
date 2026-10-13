//! Blue Fang BFB hierarchy traversal, transform composition, and reference validation.

use std::collections::HashSet;

use super::super::{
    native_geometry::source_transform_conversion::compose_blue_fang_row_major_source_matrices,
    native_source_byte_reading::{read_i32_little_endian, read_u32_little_endian, read_u8},
};
use super::{
    object_block_framing::parse_blue_fang_bfb_padded_string,
    source_error::BlueFangBfbSourceError,
    source_matrix_reading::read_blue_fang_bfb_row_major_matrix,
    source_types::{BlueFangBfbMeshBlock, BlueFangBfbNode, BlueFangBfbNodeKind},
};

const BLUE_FANG_BFB_IDENTITY_MATRIX: [[f32; 4]; 4] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

struct ParsedBlueFangBfbNode {
    block_id: u32,
    name: String,
    child_offset: Option<usize>,
    sibling_offset: Option<usize>,
    local_transform: [[f32; 4]; 4],
    kind: BlueFangBfbNodeKind,
}
pub(super) fn parse_blue_fang_bfb_hierarchy(
    bytes: &[u8],
    hierarchy_start: usize,
    source_path: &str,
) -> Result<Vec<BlueFangBfbNode>, BlueFangBfbSourceError> {
    if hierarchy_start == bytes.len() {
        return Ok(Vec::new());
    }
    if hierarchy_start > bytes.len() {
        return Err(BlueFangBfbSourceError::InvalidData {
            source_path: source_path.to_owned(),
            detail: "BFB hierarchy starts beyond end of file".to_owned(),
        });
    }
    let mut nodes = Vec::new();
    let mut visited = HashSet::new();
    parse_blue_fang_bfb_siblings(
        bytes,
        hierarchy_start,
        None,
        None,
        BLUE_FANG_BFB_IDENTITY_MATRIX,
        false,
        source_path,
        &mut visited,
        &mut nodes,
    )?;
    Ok(nodes)
}
#[allow(
    clippy::too_many_arguments,
    reason = "recursive traversal passes inherited node state and shared output"
)]
fn parse_blue_fang_bfb_siblings(
    bytes: &[u8],
    first_offset: usize,
    parent: Option<usize>,
    inherited_lod_index: Option<u32>,
    parent_transform: [[f32; 4]; 4],
    parent_is_lod_group: bool,
    source_path: &str,
    visited: &mut HashSet<usize>,
    nodes: &mut Vec<BlueFangBfbNode>,
) -> Result<(), BlueFangBfbSourceError> {
    let mut sibling_offset = Some(first_offset);
    let mut sibling_index = 0_u32;
    while let Some(offset) = sibling_offset {
        if !visited.insert(offset) {
            return Err(BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: format!("BFB hierarchy contains a cycle or repeated node at {offset}"),
            });
        }
        let parsed = parse_blue_fang_bfb_node(bytes, offset, source_path)?;
        let lod_index = parent_is_lod_group
            .then_some(sibling_index)
            .or(inherited_lod_index);
        let world_transform =
            compose_blue_fang_row_major_source_matrices(parsed.local_transform, parent_transform);
        let node_index = nodes.len();
        let is_lod_group = matches!(&parsed.kind, BlueFangBfbNodeKind::LodGroup);
        let child_offset = parsed.child_offset;
        sibling_offset = parsed.sibling_offset;
        nodes.push(BlueFangBfbNode {
            block_id: parsed.block_id,
            name: parsed.name,
            parent,
            lod_index,
            local_transform: parsed.local_transform,
            world_transform,
            kind: parsed.kind,
        });
        if let Some(child_offset) = child_offset {
            parse_blue_fang_bfb_siblings(
                bytes,
                child_offset,
                Some(node_index),
                lod_index,
                world_transform,
                is_lod_group,
                source_path,
                visited,
                nodes,
            )?;
        }
        sibling_index = sibling_index.saturating_add(1);
    }
    Ok(())
}
fn parse_blue_fang_bfb_node(
    bytes: &[u8],
    offset: usize,
    source_path: &str,
) -> Result<ParsedBlueFangBfbNode, BlueFangBfbSourceError> {
    let prefix_end =
        offset
            .checked_add(145)
            .ok_or_else(|| BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: "BFB hierarchy node offset overflow".to_owned(),
            })?;
    if prefix_end > bytes.len() {
        return Err(BlueFangBfbSourceError::InvalidData {
            source_path: source_path.to_owned(),
            detail: format!("truncated BFB hierarchy node at {offset}"),
        });
    }
    let mut cursor = offset;
    let block_id =
        read_u32_little_endian(bytes, &mut cursor, source_path, "BFB hierarchy block ID")?;
    let node_type =
        read_u32_little_endian(bytes, &mut cursor, source_path, "BFB hierarchy node type")?;
    let child_offset = read_blue_fang_bfb_link_offset(
        read_i32_little_endian(
            bytes,
            &mut cursor,
            source_path,
            "BFB hierarchy child offset",
        )?,
        bytes.len(),
        source_path,
        "child",
    )?;
    let sibling_offset = read_blue_fang_bfb_link_offset(
        read_i32_little_endian(
            bytes,
            &mut cursor,
            source_path,
            "BFB hierarchy sibling offset",
        )?,
        bytes.len(),
        source_path,
        "sibling",
    )?;
    let _unknown = read_u8(
        bytes,
        &mut cursor,
        source_path,
        "BFB hierarchy unknown field",
    )?;
    let name = parse_blue_fang_bfb_padded_string(
        &bytes[cursor..cursor + 64],
        source_path,
        "BFB hierarchy node name",
    )?;
    cursor += 64;
    let local_transform = read_blue_fang_bfb_row_major_matrix(
        bytes,
        &mut cursor,
        source_path,
        "BFB hierarchy transform",
    )?;
    let kind = match node_type {
        1 => BlueFangBfbNodeKind::Node,
        2 => BlueFangBfbNodeKind::LodGroup,
        3 => {
            let mut mesh_cursor = offset + 161;
            let mesh_block_id = read_u32_little_endian(
                bytes,
                &mut mesh_cursor,
                source_path,
                "BFB mesh-link block ID",
            )?;
            let material_start = offset + 169;
            let material_end = material_start + 128;
            let material_bytes = bytes.get(material_start..material_end).ok_or_else(|| {
                BlueFangBfbSourceError::InvalidData {
                    source_path: source_path.to_owned(),
                    detail: format!("truncated BFB mesh-link node at {offset}"),
                }
            })?;
            BlueFangBfbNodeKind::MeshLink {
                mesh_block_id,
                material_name: parse_blue_fang_bfb_padded_string(
                    material_bytes,
                    source_path,
                    "BFB mesh-link material name",
                )?,
            }
        }
        4 => {
            let mut mode_cursor = offset + 157;
            let mode =
                read_u32_little_endian(bytes, &mut mode_cursor, source_path, "BFB billboard mode")?;
            let mut mesh_cursor = offset + 185;
            let mesh_block_id = read_u32_little_endian(
                bytes,
                &mut mesh_cursor,
                source_path,
                "BFB billboard block ID",
            )?;
            let material_start = offset + 189;
            let material_end = material_start + 128;
            let material_bytes = bytes.get(material_start..material_end).ok_or_else(|| {
                BlueFangBfbSourceError::InvalidData {
                    source_path: source_path.to_owned(),
                    detail: format!("truncated BFB billboard node at {offset}"),
                }
            })?;
            BlueFangBfbNodeKind::Billboard {
                mesh_block_id,
                material_name: parse_blue_fang_bfb_padded_string(
                    material_bytes,
                    source_path,
                    "BFB billboard material name",
                )?,
                mode,
            }
        }
        5 => BlueFangBfbNodeKind::ModelJoint {
            component_name: read_blue_fang_attached_resource_name(bytes, offset, source_path)?,
        },
        6 => BlueFangBfbNodeKind::ParticleSystem {
            resource_name: read_blue_fang_attached_resource_name(bytes, offset, source_path)?,
        },
        other => BlueFangBfbNodeKind::Other(other),
    };
    Ok(ParsedBlueFangBfbNode {
        block_id,
        name,
        child_offset,
        sibling_offset,
        local_transform,
        kind,
    })
}

fn read_blue_fang_attached_resource_name(
    bytes: &[u8],
    offset: usize,
    source_path: &str,
) -> Result<String, BlueFangBfbSourceError> {
    // The common node payload is 140 bytes after the 17-byte tree header.
    // Its descriptor count is followed by that many four-byte object IDs.
    let mut cursor = offset.checked_add(153).ok_or_else(|| {
        BlueFangBfbSourceError::invalid_data(source_path, "BFB node descriptor offset overflow")
    })?;
    let descriptor_count =
        read_u32_little_endian(bytes, &mut cursor, source_path, "BFB node descriptor count")?;
    let resource_start = usize::try_from(descriptor_count)
        .ok()
        .and_then(|count| count.checked_mul(4))
        .and_then(|size| cursor.checked_add(size))
        .ok_or_else(|| {
            BlueFangBfbSourceError::invalid_data(source_path, "BFB node descriptor offset overflow")
        })?;
    let resource_end = resource_start.checked_add(64).ok_or_else(|| {
        BlueFangBfbSourceError::invalid_data(source_path, "BFB attached resource offset overflow")
    })?;
    let resource = bytes.get(resource_start..resource_end).ok_or_else(|| {
        BlueFangBfbSourceError::invalid_data(source_path, "truncated BFB attached resource name")
    })?;
    parse_blue_fang_bfb_padded_string(resource, source_path, "BFB attached resource name")
}

#[cfg(test)]
mod particle_attachment_tests {
    use super::read_blue_fang_attached_resource_name;

    #[test]
    fn particle_attachment_reads_name_after_descriptor_ids() {
        let mut bytes = vec![0_u8; 157 + 8 + 64];
        bytes[153..157].copy_from_slice(&2_u32.to_le_bytes());
        bytes[165..173].copy_from_slice(b"lampfire");
        assert_eq!(
            read_blue_fang_attached_resource_name(&bytes, 0, "test.bfb").unwrap(),
            "lampfire"
        );
    }
}
fn read_blue_fang_bfb_link_offset(
    offset: i32,
    file_len: usize,
    source_path: &str,
    field: &str,
) -> Result<Option<usize>, BlueFangBfbSourceError> {
    if offset <= 0 {
        return Ok(None);
    }
    let offset = offset as usize;
    if offset >= file_len {
        return Err(BlueFangBfbSourceError::InvalidData {
            source_path: source_path.to_owned(),
            detail: format!("BFB hierarchy {field} offset {offset} is outside the file"),
        });
    }
    Ok(Some(offset))
}
pub(super) fn validate_blue_fang_bfb_hierarchy(
    hierarchy: &[BlueFangBfbNode],
    mesh_blocks: &[BlueFangBfbMeshBlock],
    source_path: &str,
) -> Result<(), BlueFangBfbSourceError> {
    for node in hierarchy {
        let mesh_block_id = match &node.kind {
            BlueFangBfbNodeKind::MeshLink { mesh_block_id, .. }
            | BlueFangBfbNodeKind::Billboard { mesh_block_id, .. } => *mesh_block_id,
            _ => continue,
        };
        if !mesh_blocks
            .iter()
            .any(|block| block.block_id == mesh_block_id)
        {
            return Err(BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: format!(
                    "BFB hierarchy node {} references missing logical mesh block {mesh_block_id}",
                    node.block_id
                ),
            });
        }
    }
    Ok(())
}
