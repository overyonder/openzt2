//! Blue Fang BFB render-part geometry and skin lowering.

use bevy::math::Mat4;
use openzt2_game_data::AssetId;

use crate::assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;

use super::super::{
    blue_fang_bfb_source::{
        document::BlueFangBfbDocument,
        source_types::{
            BlueFangBfbBone, BlueFangBfbMesh, BlueFangBfbMeshBlock, BlueFangBfbNodeKind,
            BlueFangBfbVertex,
        },
    },
    model::{JointSource, MeshSource, ModelSource, SubmeshSource},
    native_geometry_lowering_error::{
        NativeGeometryLoweringError, NativeGeometryLoweringErrorKind, NativeGeometrySourceFamily,
    },
};
use super::{
    native_geometry_identity::{blue_fang_geometry_virtual_path, NativeMaterialReference},
    source_transform_conversion::convert_blue_fang_source_matrix_to_gltf_matrix,
    validated_vertex_topology_assembly::{
        create_validated_vertex_stream, native_geometry_lowering_error,
        validate_triangle_index_stream,
    },
};

struct SelectedBlueFangBfbRenderPart<'a> {
    node_index: usize,
    name: &'a str,
    mesh_block: &'a BlueFangBfbMeshBlock,
    mesh_data: &'a BlueFangBfbMesh,
    material_name: &'a str,
}

impl<'a> SelectedBlueFangBfbRenderPart<'a> {
    fn vertices(&self) -> &'a [BlueFangBfbVertex] {
        let vertex_start = self.mesh_block.vertex_start as usize;
        let vertex_end = vertex_start + self.mesh_block.vertex_count as usize;
        &self.mesh_data.vertices[vertex_start..vertex_end]
    }

    fn indices(&self) -> &'a [u32] {
        let index_start = self.mesh_block.index_start as usize;
        let index_end = index_start + self.mesh_block.index_count as usize;
        &self.mesh_data.indices[index_start..index_end]
    }
}

pub(in crate::assets::model_source) fn lower_blue_fang_render_parts_for_lod(
    document: &BlueFangBfbDocument,
    lod: u32,
    mut resolve_material: impl FnMut(NativeMaterialReference<'_>) -> Option<AssetId>,
) -> Result<Vec<ModelSource>, NativeGeometryLoweringError> {
    select_blue_fang_render_parts_for_lod(document, lod)
        .into_iter()
        // Retain empty mesh slots in the scene hierarchy without emitting geometry.
        .filter(|part| !(part.vertices().is_empty() && part.indices().is_empty()))
        .map(|part| adapt_bfb_part(document, part, &mut resolve_material))
        .collect()
}

fn select_blue_fang_render_parts_for_lod(
    document: &BlueFangBfbDocument,
    lod_index: u32,
) -> Vec<SelectedBlueFangBfbRenderPart<'_>> {
    document
        .hierarchy()
        .enumerate()
        .filter(|(_, node)| node.lod_index.is_none_or(|index| index == lod_index))
        .filter_map(|(node_index, node)| {
            let (mesh_block_id, material_name) = match &node.kind {
                BlueFangBfbNodeKind::MeshLink {
                    mesh_block_id,
                    material_name,
                }
                | BlueFangBfbNodeKind::Billboard {
                    mesh_block_id,
                    material_name,
                    ..
                } => (*mesh_block_id, material_name.as_str()),
                _ => return None,
            };
            let mesh_block = document
                .mesh_blocks()
                .find(|mesh_block| mesh_block.block_id == mesh_block_id)?;
            let mesh_data = document
                .meshes()
                .find(|mesh| mesh.block_id == mesh_block.mesh_data_block_id)?;
            Some(SelectedBlueFangBfbRenderPart {
                node_index,
                name: node.name.as_str(),
                mesh_block,
                mesh_data,
                material_name,
            })
        })
        .collect()
}

fn adapt_bfb_part(
    document: &BlueFangBfbDocument,
    part: SelectedBlueFangBfbRenderPart<'_>,
    resolve_material: &mut impl FnMut(NativeMaterialReference<'_>) -> Option<AssetId>,
) -> Result<ModelSource, NativeGeometryLoweringError> {
    // Both values occur in shipped BFB files. The renderer consumed
    // each stream as indexed triangles and the index counts are
    // triangular; the flag distinguishes the authored mesh class rather than
    // a Direct3D primitive topology at this boundary.
    if !matches!(part.mesh_data.primitive_kind, 2 | 3) {
        return Err(native_geometry_lowering_error(
            NativeGeometrySourceFamily::Bfb,
            &document.source_path,
            NativeGeometryLoweringErrorKind::UnsupportedPrimitive {
                primitive: part.mesh_data.primitive_kind,
            },
        ));
    }
    let material = resolve_material(NativeMaterialReference::BlueFangMaterialName(
        part.material_name,
    ))
    .ok_or_else(|| {
        native_geometry_lowering_error(
            NativeGeometrySourceFamily::Bfb,
            &document.source_path,
            NativeGeometryLoweringErrorKind::MissingMaterial {
                reference: part.material_name.to_owned(),
            },
        )
    })?;
    let source_vertices = part.vertices();
    let indices = validate_triangle_index_stream(
        part.indices(),
        source_vertices.len(),
        &document.source_path,
        NativeGeometrySourceFamily::Bfb,
    )?;
    let (retained_vertex_indices, indices) = compact_bfb_invalid_vertices(source_vertices, indices);
    if indices.is_empty() {
        return Err(native_geometry_lowering_error(
            NativeGeometrySourceFamily::Bfb,
            &document.source_path,
            NativeGeometryLoweringErrorKind::InvalidTopology {
                detail: "no triangles remain after authored absent vertices are removed",
            },
        ));
    }
    let mut vertices = create_validated_vertex_stream(
        retained_vertex_indices
            .iter()
            .map(|index| source_vertices[*index].position),
        retained_vertex_indices
            .iter()
            .map(|index| source_vertices[*index].normal),
        retained_vertex_indices
            .iter()
            .map(|index| source_vertices[*index].diffuse.map(bfb_vertex_color)),
        retained_vertex_indices.iter().map(|index| {
            std::array::from_fn(|set| {
                source_vertices[*index]
                    .texture_coordinates
                    .get(set)
                    .copied()
            })
        }),
        retained_vertex_indices.iter().map(|index| {
            source_vertices[*index]
                .auxiliary_vector
                .map(convert_source_z_up_vector_to_bevy_y_up_coordinates)
        }),
        &indices,
        &document.source_path,
        NativeGeometrySourceFamily::Bfb,
    )?;
    let (skeleton, joints) = match part.mesh_block.skin.as_ref() {
        None => (AssetId::default(), Vec::new()),
        Some(skin) => {
            if skin.vertex_influences.len() != source_vertices.len() {
                return Err(native_geometry_lowering_error(
                    NativeGeometrySourceFamily::Bfb,
                    &document.source_path,
                    NativeGeometryLoweringErrorKind::SkeletonLayoutMissing {
                        geometry: part.name.to_owned(),
                    },
                ));
            }
            if skin.bones.iter().enumerate().any(|(index, bone)| {
                skin.bones
                    .iter()
                    .position(|candidate| candidate.identity[0] == bone.identity[1])
                    .is_some_and(|parent| parent >= index)
            }) {
                return Err(native_geometry_lowering_error(
                    NativeGeometrySourceFamily::Bfb,
                    &document.source_path,
                    NativeGeometryLoweringErrorKind::SkeletonLayoutMissing {
                        geometry: part.name.to_owned(),
                    },
                ));
            }
            vertices
                .iter_mut()
                .zip(&retained_vertex_indices)
                .for_each(|(vertex, source_index)| {
                    let influence = &skin.vertex_influences[*source_index];
                    vertex.joints = Some(influence.joint_indices);
                    vertex.weights = Some(normalize_weights(influence.joint_weights));
                });
            let joints = reconstruct_blue_fang_bfb_bind_pose_joint_transforms(
                document,
                part.name,
                &skin.bones,
            )?;
            (
                AssetId::from_key(&format!(
                    "{}.__skeleton/bfb_{:08x}",
                    document.source_path.as_str(),
                    part.node_index
                )),
                joints,
            )
        }
    };
    if part.mesh_block.skin.is_some() && joints.is_empty() {
        return Err(native_geometry_lowering_error(
            NativeGeometrySourceFamily::Bfb,
            &document.source_path,
            NativeGeometryLoweringErrorKind::SkeletonLayoutMissing {
                geometry: part.name.to_owned(),
            },
        ));
    }
    let index_count = indices.len() as u32;
    Ok(ModelSource {
        virtual_path: blue_fang_geometry_virtual_path(document, part.node_index),
        meshes: vec![MeshSource {
            vertices,
            indices,
            submeshes: vec![SubmeshSource {
                first_index: 0,
                index_count,
                material,
                native_material: None,
                flat_shaded: false,
            }],
            lods: Vec::new(),
        }],
        skeleton,
        joint_hierarchy_root: None,
        joints,
    })
}

fn reconstruct_blue_fang_bfb_bind_pose_joint_transforms(
    document: &BlueFangBfbDocument,
    geometry_name: &str,
    bones: &[BlueFangBfbBone],
) -> Result<Vec<JointSource>, NativeGeometryLoweringError> {
    // The BFB table stores each bone's local bind transform. The shipped BF
    // clips independently encode the same local translations (for example,
    // the Dromedary Spine1, Neck, and Head keys). glTF needs those transforms
    // on the joint nodes and the inverse of their accumulated global bind
    // transforms in the skin accessor.
    let local_bind_matrices = bones
        .iter()
        .map(|bone| {
            Mat4::from_cols_array(&convert_blue_fang_source_matrix_to_gltf_matrix(
                bone.transform,
            ))
        })
        .collect::<Vec<_>>();
    let parent_indices = bones
        .iter()
        .enumerate()
        .map(|(index, bone)| {
            bones[..index]
                .iter()
                .position(|candidate| candidate.identity[0] == bone.identity[1])
        })
        .collect::<Vec<_>>();
    let mut global_bind_matrices = Vec::with_capacity(bones.len());
    for (index, parent) in parent_indices.iter().copied().enumerate() {
        global_bind_matrices.push(parent.map_or(local_bind_matrices[index], |parent| {
            global_bind_matrices[parent] * local_bind_matrices[index]
        }));
    }
    let inverse_bind_matrices = global_bind_matrices
        .iter()
        .map(|global_bind_matrix| global_bind_matrix.inverse())
        .collect::<Vec<_>>();
    if inverse_bind_matrices
        .iter()
        .any(|matrix| !matrix.is_finite())
    {
        return Err(native_geometry_lowering_error(
            NativeGeometrySourceFamily::Bfb,
            &document.source_path,
            NativeGeometryLoweringErrorKind::SkeletonLayoutMissing {
                geometry: geometry_name.to_owned(),
            },
        ));
    }
    bones
        .iter()
        .enumerate()
        .map(|(index, bone)| {
            let (local_scale, local_rotation, local_translation) =
                local_bind_matrices[index].to_scale_rotation_translation();
            if !local_scale.is_finite()
                || !local_rotation.is_finite()
                || !local_translation.is_finite()
            {
                return Err(native_geometry_lowering_error(
                    NativeGeometrySourceFamily::Bfb,
                    &document.source_path,
                    NativeGeometryLoweringErrorKind::SkeletonLayoutMissing {
                        geometry: geometry_name.to_owned(),
                    },
                ));
            }
            Ok(JointSource {
                name: bone.name.clone(),
                parent: parent_indices[index].and_then(|parent| u16::try_from(parent).ok()),
                local_translation: local_translation.to_array(),
                local_rotation_xyzw: local_rotation.to_array(),
                local_scale: local_scale.to_array(),
                inverse_bind: inverse_bind_matrices[index].to_cols_array(),
            })
        })
        .collect()
}

/// Some shipped BFB streams contain uninitialized `0xff` spans which cross
/// vertex-record boundaries. Direct3D clipped triangles whose declared
/// attributes decoded to NaN; native GPU buffers must not contain them.
/// Remove every vertex with an unusable declared position, normal, or texture
/// coordinate, discard only triangles which touch those vertices, and remap
/// the remaining source vertices. No replacement geometry or attributes are
/// invented.
fn compact_bfb_invalid_vertices(
    vertices: &[BlueFangBfbVertex],
    indices: Vec<u32>,
) -> (Vec<usize>, Vec<u32>) {
    let retained = vertices
        .iter()
        .map(has_finite_bfb_attributes)
        .collect::<Vec<_>>();
    if retained.iter().all(|retained| *retained) {
        return ((0..vertices.len()).collect(), indices);
    }

    let mut next = 0_u32;
    let remap = retained
        .iter()
        .map(|retained| {
            retained.then(|| {
                let current = next;
                next += 1;
                current
            })
        })
        .collect::<Vec<_>>();
    let indices = indices
        .chunks_exact(3)
        .filter(|triangle| {
            triangle
                .iter()
                .all(|index| remap[*index as usize].is_some())
        })
        .flat_map(|triangle| triangle.iter().map(|index| remap[*index as usize].unwrap()))
        .collect();
    let retained_indices = vertices
        .iter()
        .enumerate()
        .zip(retained)
        .filter_map(|((index, _), retained)| retained.then_some(index))
        .collect();
    (retained_indices, indices)
}

fn has_finite_bfb_attributes(vertex: &BlueFangBfbVertex) -> bool {
    vertex
        .position
        .iter()
        .all(|component| component.is_finite())
        && vertex
            .normal
            .is_none_or(|normal| normal.iter().all(|component| component.is_finite()))
        && vertex
            .texture_coordinates
            .iter()
            .all(|uv| uv.iter().all(|component| component.is_finite()))
}

fn bfb_vertex_color(argb: u32) -> [f32; 4] {
    // The BFB `D` declaration is a Direct3D D3DCOLOR: 8-bit ARGB in the
    // integer value. Convert it to normalized RGBA components.
    [
        ((argb >> 16) & 0xff) as f32 / 255.0,
        ((argb >> 8) & 0xff) as f32 / 255.0,
        (argb & 0xff) as f32 / 255.0,
        ((argb >> 24) & 0xff) as f32 / 255.0,
    ]
}

fn normalize_weights(mut weights: [f32; 4]) -> [f32; 4] {
    weights.iter_mut().for_each(|weight| {
        if !weight.is_finite() || *weight < 0.0 {
            *weight = 0.0;
        }
    });
    let sum = weights.iter().sum::<f32>();
    if sum <= f32::EPSILON {
        [1.0, 0.0, 0.0, 0.0]
    } else {
        weights.map(|weight| weight / sum)
    }
}
