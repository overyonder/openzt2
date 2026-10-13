//! Blue Fang BFB mesh streams, logical mesh blocks, skin tables, and relationship validation.

use std::mem::size_of;

use super::super::native_source_byte_reading::{
    read_f32_little_endian, read_three_component_vector, read_u16_little_endian,
    read_u32_little_endian, read_u8,
};
use super::{
    object_block_framing::{parse_blue_fang_bfb_padded_string, BlueFangBfbObjectBlock},
    source_error::BlueFangBfbSourceError,
    source_matrix_reading::read_blue_fang_bfb_row_major_matrix,
    source_types::{
        BlueFangBfbBone, BlueFangBfbMesh, BlueFangBfbMeshBlock, BlueFangBfbSkin, BlueFangBfbVertex,
        BlueFangBfbVertexInfluence,
    },
};

pub(super) fn parse_blue_fang_bfb_mesh(
    bytes: &[u8],
    block: &BlueFangBfbObjectBlock,
    source_path: &str,
) -> Result<BlueFangBfbMesh, BlueFangBfbSourceError> {
    let format_start =
        block
            .content_start
            .checked_add(1)
            .ok_or_else(|| BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: "BFB format offset overflow".to_owned(),
            })?;
    let format_end =
        format_start
            .checked_add(64)
            .ok_or_else(|| BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: "BFB format end overflow".to_owned(),
            })?;
    let format_bytes =
        bytes
            .get(format_start..format_end)
            .ok_or_else(|| BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: "truncated BFB vertex format name".to_owned(),
            })?;
    let vertex_format =
        parse_blue_fang_bfb_padded_string(format_bytes, source_path, "BFB vertex format")?;
    let mut cursor = format_end;
    let stride_bytes =
        read_u32_little_endian(bytes, &mut cursor, source_path, "BFB vertex stride")?;
    let vertex_count = read_u32_little_endian(bytes, &mut cursor, source_path, "BFB vertex count")?;
    let stride =
        usize::try_from(stride_bytes).map_err(|_| BlueFangBfbSourceError::InvalidData {
            source_path: source_path.to_owned(),
            detail: "BFB vertex stride does not fit usize".to_owned(),
        })?;
    let vertex_count_usize =
        usize::try_from(vertex_count).map_err(|_| BlueFangBfbSourceError::InvalidData {
            source_path: source_path.to_owned(),
            detail: "BFB vertex count does not fit usize".to_owned(),
        })?;
    if stride < 12 {
        return Err(BlueFangBfbSourceError::InvalidData {
            source_path: source_path.to_owned(),
            detail: format!("BFB vertex stride {stride} is too small for positions"),
        });
    }
    let vertex_data_len = stride.checked_mul(vertex_count_usize).ok_or_else(|| {
        BlueFangBfbSourceError::InvalidData {
            source_path: source_path.to_owned(),
            detail: "BFB vertex data length overflow".to_owned(),
        }
    })?;
    let vertex_data_end =
        cursor
            .checked_add(vertex_data_len)
            .ok_or_else(|| BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: "BFB vertex data end overflow".to_owned(),
            })?;
    if vertex_data_end > block.block_end {
        return Err(BlueFangBfbSourceError::InvalidData {
            source_path: source_path.to_owned(),
            detail: "BFB vertex stream extends beyond mesh block".to_owned(),
        });
    }
    let mut vertices = Vec::with_capacity(vertex_count_usize);
    for index in 0..vertex_count_usize {
        let vertex_start = cursor
            .checked_add(index.checked_mul(stride).ok_or_else(|| {
                BlueFangBfbSourceError::InvalidData {
                    source_path: source_path.to_owned(),
                    detail: "BFB vertex offset overflow".to_owned(),
                }
            })?)
            .ok_or_else(|| BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: "BFB vertex offset overflow".to_owned(),
            })?;
        vertices.push(parse_blue_fang_bfb_vertex(
            bytes,
            vertex_start,
            stride,
            &vertex_format,
            source_path,
        )?);
    }
    cursor = vertex_data_end;
    let primitive_kind = read_u8(bytes, &mut cursor, source_path, "BFB primitive kind")?;
    let index_count = read_u32_little_endian(bytes, &mut cursor, source_path, "BFB index count")?;
    let index_count_usize =
        usize::try_from(index_count).map_err(|_| BlueFangBfbSourceError::InvalidData {
            source_path: source_path.to_owned(),
            detail: "BFB index count does not fit usize".to_owned(),
        })?;
    let index_data_len =
        index_count_usize
            .checked_mul(2)
            .ok_or_else(|| BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: "BFB index data length overflow".to_owned(),
            })?;
    if cursor.saturating_add(index_data_len) > block.block_end {
        return Err(BlueFangBfbSourceError::InvalidData {
            source_path: source_path.to_owned(),
            detail: "BFB index stream extends beyond mesh block".to_owned(),
        });
    }
    let mut indices = Vec::with_capacity(index_count_usize);
    for _ in 0..index_count {
        indices.push(read_u16_little_endian(bytes, &mut cursor, source_path, "BFB index")? as u32);
    }
    Ok(BlueFangBfbMesh {
        block_id: block.block_id,
        block_type: block.block_type,
        name: block.name.clone(),
        vertex_format,
        stride_bytes,
        primitive_kind,
        vertices,
        indices,
    })
}
pub(super) fn parse_blue_fang_bfb_mesh_block(
    bytes: &[u8],
    block: &BlueFangBfbObjectBlock,
    source_path: &str,
) -> Result<BlueFangBfbMeshBlock, BlueFangBfbSourceError> {
    let mut cursor =
        block
            .content_start
            .checked_add(1)
            .ok_or_else(|| BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: "BFB logical mesh offset overflow".to_owned(),
            })?;
    let payload_end =
        cursor
            .checked_add(44)
            .ok_or_else(|| BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: "BFB logical mesh payload overflow".to_owned(),
            })?;
    if payload_end > block.block_end {
        return Err(BlueFangBfbSourceError::InvalidData {
            source_path: source_path.to_owned(),
            detail: "truncated BFB logical mesh block".to_owned(),
        });
    }
    let mesh_data_block_id =
        read_u32_little_endian(bytes, &mut cursor, source_path, "BFB mesh-data block ID")?;
    let _unknown = read_u32_little_endian(
        bytes,
        &mut cursor,
        source_path,
        "BFB logical mesh unknown field",
    )?;
    let index_start = read_u32_little_endian(
        bytes,
        &mut cursor,
        source_path,
        "BFB logical mesh index start",
    )?;
    let index_count = read_u32_little_endian(
        bytes,
        &mut cursor,
        source_path,
        "BFB logical mesh index count",
    )?;
    let vertex_start = read_u32_little_endian(
        bytes,
        &mut cursor,
        source_path,
        "BFB logical mesh vertex start",
    )?;
    let vertex_count = read_u32_little_endian(
        bytes,
        &mut cursor,
        source_path,
        "BFB logical mesh vertex count",
    )?;
    let face_count = read_u32_little_endian(
        bytes,
        &mut cursor,
        source_path,
        "BFB logical mesh face count",
    )?;
    let bounds_center = read_three_component_vector(
        bytes,
        &mut cursor,
        source_path,
        "BFB logical mesh bounds center",
    )?;
    let bounds_radius = read_f32_little_endian(
        bytes,
        &mut cursor,
        source_path,
        "BFB logical mesh bounds radius",
    )?;
    let skin = (block.block_type == 8)
        .then(|| parse_blue_fang_bfb_skin(bytes, payload_end, block.block_end, source_path))
        .transpose()?;
    Ok(BlueFangBfbMeshBlock {
        block_id: block.block_id,
        block_type: block.block_type,
        name: block.name.clone(),
        mesh_data_block_id,
        index_start,
        index_count,
        vertex_start,
        vertex_count,
        face_count,
        bounds_center,
        bounds_radius,
        skin,
    })
}
fn parse_blue_fang_bfb_skin(
    bytes: &[u8],
    start: usize,
    end: usize,
    source_path: &str,
) -> Result<BlueFangBfbSkin, BlueFangBfbSourceError> {
    let mut cursor = start;
    let bone_count =
        read_u32_little_endian(bytes, &mut cursor, source_path, "BFB skin bone count")?;
    let vertex_count =
        read_u32_little_endian(bytes, &mut cursor, source_path, "BFB skin vertex count")?;
    let mut bones = Vec::with_capacity(bone_count as usize);
    for _ in 0..bone_count {
        if cursor.saturating_add(131) > end {
            return Err(BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: "truncated BFB skin bone table".to_owned(),
            });
        }
        let identity = [bytes[cursor], bytes[cursor + 1], bytes[cursor + 2]];
        cursor += 3;
        let name = parse_blue_fang_bfb_padded_string(
            &bytes[cursor..cursor + 64],
            source_path,
            "BFB skin bone name",
        )?;
        cursor += 64;
        let transform = read_blue_fang_bfb_row_major_matrix(
            bytes,
            &mut cursor,
            source_path,
            "BFB skin bone transform",
        )?;
        bones.push(BlueFangBfbBone {
            identity,
            name,
            transform,
        });
    }
    let influence_bytes = usize::try_from(vertex_count)
        .ok()
        .and_then(|count| count.checked_mul(16))
        .ok_or_else(|| BlueFangBfbSourceError::InvalidData {
            source_path: source_path.to_owned(),
            detail: "BFB skin influence table size overflows".to_owned(),
        })?;
    if cursor.saturating_add(influence_bytes) > end {
        return Err(BlueFangBfbSourceError::InvalidData {
            source_path: source_path.to_owned(),
            detail: "truncated BFB skin influence table".to_owned(),
        });
    }
    let mut vertex_influences = Vec::with_capacity(vertex_count as usize);
    for _ in 0..vertex_count {
        let raw_indices = [
            bytes[cursor],
            bytes[cursor + 1],
            bytes[cursor + 2],
            bytes[cursor + 3],
        ];
        cursor += 4;
        let first = read_f32_little_endian(
            bytes,
            &mut cursor,
            source_path,
            "BFB skin first joint weight",
        )?;
        let second = read_f32_little_endian(
            bytes,
            &mut cursor,
            source_path,
            "BFB skin second joint weight",
        )?;
        let third = read_f32_little_endian(
            bytes,
            &mut cursor,
            source_path,
            "BFB skin third joint weight",
        )?;
        let fourth = (1.0 - first - second - third).max(0.0);
        let mut joint_weights = [first, second, third, fourth];
        raw_indices
            .iter()
            .zip(joint_weights.iter_mut())
            .filter(|(index, _)| **index == u8::MAX)
            .for_each(|(_, weight)| *weight = 0.0);
        vertex_influences.push(BlueFangBfbVertexInfluence {
            joint_indices: raw_indices.map(|index| {
                if index == u8::MAX {
                    0
                } else {
                    u16::from(index)
                }
            }),
            joint_weights,
        });
    }
    Ok(BlueFangBfbSkin {
        vertex_count,
        bones,
        vertex_influences,
    })
}
pub(super) fn validate_blue_fang_bfb_mesh_blocks(
    mesh_blocks: &[BlueFangBfbMeshBlock],
    meshes: &[BlueFangBfbMesh],
    source_path: &str,
) -> Result<(), BlueFangBfbSourceError> {
    for block in mesh_blocks {
        let mesh = meshes
            .iter()
            .find(|mesh| mesh.block_id == block.mesh_data_block_id)
            .ok_or_else(|| BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: format!(
                    "BFB logical mesh block {} references missing mesh-data block {}",
                    block.block_id, block.mesh_data_block_id
                ),
            })?;
        let vertex_end = block
            .vertex_start
            .checked_add(block.vertex_count)
            .ok_or_else(|| BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: format!(
                    "BFB logical mesh block {} vertex range overflows",
                    block.block_id
                ),
            })? as usize;
        if vertex_end > mesh.vertices.len() {
            return Err(BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: format!(
                    "BFB logical mesh block {} vertex range {}..{} exceeds mesh-data block {} vertex count {}",
                    block.block_id,
                    block.vertex_start,
                    vertex_end,
                    mesh.block_id,
                    mesh.vertices.len()
                ),
            });
        }
        let index_start = block.index_start as usize;
        let index_end = block
            .index_start
            .checked_add(block.index_count)
            .ok_or_else(|| BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: format!(
                    "BFB logical mesh block {} index range overflows",
                    block.block_id
                ),
            })? as usize;
        let indices = mesh.indices.get(index_start..index_end).ok_or_else(|| {
            BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: format!(
                    "BFB logical mesh block {} index range {}..{} exceeds mesh-data block {} index count {}",
                    block.block_id,
                    block.index_start,
                    index_end,
                    mesh.block_id,
                    mesh.indices.len()
                ),
            }
        })?;
        if let Some(index) = indices
            .iter()
            .copied()
            .find(|index| *index >= block.vertex_count)
        {
            return Err(BlueFangBfbSourceError::InvalidData {
                source_path: source_path.to_owned(),
                detail: format!(
                    "BFB logical mesh block {} index {index} exceeds its vertex count {}",
                    block.block_id, block.vertex_count
                ),
            });
        }
    }
    Ok(())
}
fn parse_blue_fang_bfb_vertex(
    bytes: &[u8],
    vertex_start: usize,
    stride: usize,
    vertex_format: &str,
    source_path: &str,
) -> Result<BlueFangBfbVertex, BlueFangBfbSourceError> {
    let mut cursor = vertex_start;
    let position =
        read_three_component_vector(bytes, &mut cursor, source_path, "BFB vertex position")?;
    let normal = if vertex_format.starts_with("BFRVertexPN") && stride >= 24 {
        Some(read_three_component_vector(
            bytes,
            &mut cursor,
            source_path,
            "BFB vertex normal",
        )?)
    } else {
        None
    };
    // A bare `D` is the packed diffuse-colour stream. Numbered `D` semantics
    // belong to declarations such as `T3D1` and are read after the texture
    // coordinates as three-component auxiliary vectors.
    let has_diffuse = vertex_format.match_indices('D').any(|(index, _)| {
        !vertex_format[index + 1..].starts_with(|value: char| value.is_ascii_digit())
    });
    let diffuse = if has_diffuse && stride >= cursor - vertex_start + 4 {
        let mut diffuse_cursor = cursor;
        let diffuse = read_u32_little_endian(
            bytes,
            &mut diffuse_cursor,
            source_path,
            "BFB vertex diffuse",
        )?;
        cursor = diffuse_cursor;
        Some(diffuse)
    } else {
        None
    };
    // The renderer registry distinguishes ordinary two-component `T0`..
    // streams from three-component `T30`.. streams. The latter are used by
    // shipped wind-vertex foliage: XY remains the sampled texture coordinate
    // while Z is an authored wind/side-fade value which must survive lowering.
    let texture_coordinate_dimensions = (0..=3)
        .map(|index| {
            if vertex_format.contains(&format!("T3{index}")) {
                Some(3)
            } else if vertex_format.contains(&format!("T{index}")) {
                Some(2)
            } else {
                None
            }
        })
        .take_while(Option::is_some)
        .flatten()
        .collect::<Vec<_>>();
    let mut texture_coordinates = Vec::with_capacity(texture_coordinate_dimensions.len());
    for (index, dimensions) in texture_coordinate_dimensions.into_iter().enumerate() {
        if stride < cursor - vertex_start + dimensions * size_of::<f32>() {
            break;
        }
        let mut uv = [
            read_f32_little_endian(
                bytes,
                &mut cursor,
                source_path,
                &format!("BFB vertex tex{index} u"),
            )?,
            read_f32_little_endian(
                bytes,
                &mut cursor,
                source_path,
                &format!("BFB vertex tex{index} v"),
            )?,
            0.0,
        ];
        if dimensions == 3 {
            uv[2] = read_f32_little_endian(
                bytes,
                &mut cursor,
                source_path,
                &format!("BFB vertex tex{index} effect"),
            )?;
        }
        texture_coordinates.push(uv);
    }
    let tex0 = texture_coordinates.first().map(|uv| [uv[0], uv[1]]);
    let auxiliary_vector = (0..=3)
        .find(|index| vertex_format.contains(&format!("T3D{index}")))
        .filter(|_| stride >= cursor - vertex_start + 3 * size_of::<f32>())
        .map(|index| {
            read_three_component_vector(
                bytes,
                &mut cursor,
                source_path,
                &format!("BFB vertex auxiliary vector D{index}"),
            )
        })
        .transpose()?;
    Ok(BlueFangBfbVertex {
        position,
        normal,
        diffuse,
        tex0,
        texture_coordinates,
        auxiliary_vector,
    })
}
