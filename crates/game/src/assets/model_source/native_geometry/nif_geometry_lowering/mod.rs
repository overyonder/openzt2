//! NetImmerse NIF geometry, topology, shading, and render-record lowering.

use openzt2_game_data::AssetId;

use super::super::{
    model::{MeshSource, ModelSource, SubmeshSource, VertexSource},
    native_geometry_lowering_error::{
        NativeGeometryLoweringError, NativeGeometryLoweringErrorKind, NativeGeometrySourceFamily,
    },
    netimmerse_bone_level_of_detail_selection::NetImmerseSelectedBoneLevelOfDetail,
    netimmerse_nif_source::{
        block_payload::NetImmerseNifBlockPayload,
        document_source_types::{NetImmerseNifBlock, NetImmerseNifDocument},
        scene_object_source_types::NetImmerseNiGeometry,
    },
};
use super::{
    native_geometry_identity::{netimmerse_geometry_virtual_path, NativeMaterialReference},
    nif_material_lowering::{
        inherited_netimmerse_property_references, lower_netimmerse_native_material,
    },
    nif_skin_lowering::lower_netimmerse_skin,
    validated_vertex_topology_assembly::{
        create_validated_vertex_stream, native_geometry_lowering_error, strips_to_triangles,
        validate_triangle_index_stream,
    },
};

pub(in crate::assets::model_source) fn lower_netimmerse_geometry_block(
    document: &NetImmerseNifDocument,
    selected_bone_level_of_detail: &NetImmerseSelectedBoneLevelOfDetail,
    block: u32,
    mut resolve_material: impl FnMut(NativeMaterialReference<'_>) -> Option<AssetId>,
    mut resolve_texture: impl FnMut(&str) -> Option<String>,
) -> Result<ModelSource, NativeGeometryLoweringError> {
    let source_block = document.block(block as i32).ok_or_else(|| {
        native_geometry_lowering_error(
            NativeGeometrySourceFamily::Nif,
            &document.source_path,
            NativeGeometryLoweringErrorKind::MissingGeometry { block },
        )
    })?;
    let (geometry, data, source_indices) = match &source_block.payload {
        NetImmerseNifBlockPayload::NiTriShape(shape) => {
            let data = netimmerse_geometry_data_block(document, shape.geometry.data_ref, block)?;
            let NetImmerseNifBlockPayload::NiTriShapeData(data) = &data.payload else {
                return Err(missing_netimmerse_geometry(document, block));
            };
            let indices = data
                .triangles
                .as_ref()
                .ok_or_else(|| missing_netimmerse_geometry(document, block))?
                .iter()
                .flat_map(|triangle| triangle.iter().copied().map(u32::from))
                .collect::<Vec<_>>();
            (&shape.geometry, &data.geometry, indices)
        }
        NetImmerseNifBlockPayload::NiTriStrips(strips) => {
            let data = netimmerse_geometry_data_block(document, strips.geometry.data_ref, block)?;
            let NetImmerseNifBlockPayload::NiTriStripsData(data) = &data.payload else {
                return Err(missing_netimmerse_geometry(document, block));
            };
            let indices = strips_to_triangles(data.points.as_deref().unwrap_or_default());
            (&strips.geometry, &data.geometry, indices)
        }
        _ => return Err(missing_netimmerse_geometry(document, block)),
    };
    let positions = data
        .vertices
        .as_ref()
        .ok_or_else(|| missing_netimmerse_geometry(document, block))?;
    let indices = validate_triangle_index_stream(
        &source_indices,
        positions.len(),
        &document.source_path,
        NativeGeometrySourceFamily::Nif,
    )?;
    let vertex_colors_enabled =
        inherited_netimmerse_property_references(document, block, geometry)?
            .into_iter()
            .filter_map(|reference| document.block(reference))
            .find_map(|property| match &property.payload {
                NetImmerseNifBlockPayload::NiVertexColorProperty(value) => {
                    Some(value.vertex_mode != 0)
                }
                _ => None,
            })
            .unwrap_or(false);
    let mut vertices = create_validated_vertex_stream(
        positions.iter().copied(),
        positions.iter().enumerate().map(|(index, _)| {
            data.normals
                .as_ref()
                .and_then(|normals| normals.get(index))
                .copied()
        }),
        positions.iter().enumerate().map(|(index, _)| {
            vertex_colors_enabled
                .then(|| {
                    data.vertex_colors
                        .as_ref()
                        .and_then(|colors| colors.get(index))
                        .copied()
                })
                .flatten()
        }),
        positions.iter().enumerate().map(|(index, _)| {
            std::array::from_fn(|set| {
                data.uv_sets
                    .get(set)
                    .and_then(|uvs| uvs.get(index))
                    .copied()
                    .map(|uv| [uv[0], uv[1], 0.0])
            })
        }),
        std::iter::repeat_n(None, positions.len()),
        &indices,
        &document.source_path,
        NativeGeometrySourceFamily::Nif,
    )?;
    let selected_skin_instance = selected_bone_level_of_detail.selected_shape_skin_instance(
        i32::try_from(block).unwrap_or(i32::MAX),
        geometry.skin_instance_ref,
    );
    let (skeleton, joint_hierarchy_root, joints) = match selected_skin_instance {
        Some(skin_instance) if skin_instance >= 0 => {
            lower_netimmerse_skin(document, skin_instance, &mut vertices, block)?
        }
        _ => (AssetId::default(), None, Vec::new()),
    };
    let material = resolve_material(NativeMaterialReference::NetImmerseGeometry {
        document: &document.source_path,
        block,
    })
    .ok_or_else(|| {
        native_geometry_lowering_error(
            NativeGeometrySourceFamily::Nif,
            &document.source_path,
            NativeGeometryLoweringErrorKind::MissingMaterial {
                reference: format!("NIF geometry block {block}"),
            },
        )
    })?;
    let flat_shaded = netimmerse_geometry_is_flat_shaded(document, geometry)?;
    let native_material =
        lower_netimmerse_native_material(document, block, geometry, &mut resolve_texture)?;
    let (vertices, indices) = if flat_shaded {
        flat_shade(vertices, &indices)
    } else {
        (vertices, indices)
    };

    let index_count = indices.len() as u32;
    Ok(ModelSource {
        virtual_path: netimmerse_geometry_virtual_path(document, block),
        meshes: vec![MeshSource {
            vertices,
            indices,
            submeshes: vec![SubmeshSource {
                first_index: 0,
                index_count,
                material,
                native_material,
                flat_shaded,
            }],
            lods: Vec::new(),
        }],
        skeleton,
        joint_hierarchy_root,
        joints,
    })
}

fn flat_shade(vertices: Vec<VertexSource>, indices: &[u32]) -> (Vec<VertexSource>, Vec<u32>) {
    let mut output = Vec::with_capacity(indices.len());
    for triangle in indices.chunks_exact(3) {
        let [a, b, c] = [
            vertices[triangle[0] as usize],
            vertices[triangle[1] as usize],
            vertices[triangle[2] as usize],
        ];
        let edge_a: [f32; 3] = std::array::from_fn(|axis| b.position[axis] - a.position[axis]);
        let edge_b: [f32; 3] = std::array::from_fn(|axis| c.position[axis] - a.position[axis]);
        let cross = [
            edge_a[1] * edge_b[2] - edge_a[2] * edge_b[1],
            edge_a[2] * edge_b[0] - edge_a[0] * edge_b[2],
            edge_a[0] * edge_b[1] - edge_a[1] * edge_b[0],
        ];
        let length = cross
            .into_iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        let normal = if length > f32::EPSILON {
            cross.map(|value| value / length)
        } else {
            a.normal
        };
        let tangent = flat_tangent(a, b, c, normal);
        output.extend([a, b, c].map(|mut vertex| {
            vertex.normal = normal;
            vertex.tangent = tangent;
            vertex
        }));
    }
    let indices = (0..output.len())
        .map(|index| u32::try_from(index).expect("source vertex count already fits u32"))
        .collect();
    (output, indices)
}

fn flat_tangent(a: VertexSource, b: VertexSource, c: VertexSource, normal: [f32; 3]) -> [f32; 4] {
    let Some((uv_a, uv_b, uv_c)) = a.uvs[0]
        .zip(b.uvs[0])
        .zip(c.uvs[0])
        .map(|((a, b), c)| (a, b, c))
    else {
        return a.tangent;
    };
    let edge_a = std::array::from_fn::<_, 3, _>(|axis| b.position[axis] - a.position[axis]);
    let edge_b = std::array::from_fn::<_, 3, _>(|axis| c.position[axis] - a.position[axis]);
    let delta_a = [uv_b[0] - uv_a[0], uv_b[1] - uv_a[1]];
    let delta_b = [uv_c[0] - uv_a[0], uv_c[1] - uv_a[1]];
    let determinant = delta_a[0] * delta_b[1] - delta_a[1] * delta_b[0];
    if determinant.abs() <= f32::EPSILON {
        return a.tangent;
    }
    let tangent = std::array::from_fn::<_, 3, _>(|axis| {
        (edge_a[axis] * delta_b[1] - edge_b[axis] * delta_a[1]) / determinant
    });
    let length = tangent
        .into_iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    if length <= f32::EPSILON {
        return a.tangent;
    }
    let tangent = tangent.map(|value| value / length);
    let cross = [
        normal[1] * tangent[2] - normal[2] * tangent[1],
        normal[2] * tangent[0] - normal[0] * tangent[2],
        normal[0] * tangent[1] - normal[1] * tangent[0],
    ];
    let bitangent = std::array::from_fn::<_, 3, _>(|axis| {
        (edge_b[axis] * delta_a[0] - edge_a[axis] * delta_b[0]) / determinant
    });
    let handedness = if cross
        .into_iter()
        .zip(bitangent)
        .map(|(a, b)| a * b)
        .sum::<f32>()
        < 0.0
    {
        -1.0
    } else {
        1.0
    };
    [tangent[0], tangent[1], tangent[2], handedness]
}

fn netimmerse_geometry_is_flat_shaded(
    document: &NetImmerseNifDocument,
    geometry: &NetImmerseNiGeometry,
) -> Result<bool, NativeGeometryLoweringError> {
    let mut shade = None;
    for property_ref in &geometry.av_object.property_refs {
        let property = document.block(*property_ref).ok_or_else(|| {
            native_geometry_lowering_error(
                NativeGeometrySourceFamily::Nif,
                &document.source_path,
                NativeGeometryLoweringErrorKind::InvalidVertexData {
                    detail: "geometry references an absent property block",
                },
            )
        })?;
        if let NetImmerseNifBlockPayload::NiShadeProperty(property) = &property.payload {
            if shade.replace(property).is_some() {
                return Err(native_geometry_lowering_error(
                    NativeGeometrySourceFamily::Nif,
                    &document.source_path,
                    NativeGeometryLoweringErrorKind::InvalidVertexData {
                        detail: "geometry has duplicate NiShadeProperty blocks",
                    },
                ));
            }
        }
    }
    Ok(shade.is_some_and(|property| (property.flags & 1) == 0))
}

fn netimmerse_geometry_data_block<'a>(
    document: &'a NetImmerseNifDocument,
    data_ref: i32,
    geometry_block: u32,
) -> Result<&'a NetImmerseNifBlock, NativeGeometryLoweringError> {
    document
        .block(data_ref)
        .ok_or_else(|| missing_netimmerse_geometry(document, geometry_block))
}

pub(super) fn missing_netimmerse_geometry(
    document: &NetImmerseNifDocument,
    block: u32,
) -> NativeGeometryLoweringError {
    native_geometry_lowering_error(
        NativeGeometrySourceFamily::Nif,
        &document.source_path,
        NativeGeometryLoweringErrorKind::MissingGeometry { block },
    )
}
