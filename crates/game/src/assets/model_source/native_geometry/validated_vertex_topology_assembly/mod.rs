//! Shared topology validation and vertex-stream assembly for native geometry.

use crate::assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;

use super::super::{
    model::VertexSource,
    native_geometry_lowering_error::{
        NativeGeometryLoweringError, NativeGeometryLoweringErrorKind, NativeGeometrySourceFamily,
    },
};
use super::derived_normal_and_tangent_calculation::{
    derive_normals, derive_tangents, normalized_or,
};

pub(super) fn strips_to_triangles(strips: &[Vec<u16>]) -> Vec<u32> {
    strips
        .iter()
        .flat_map(|strip| {
            strip
                .windows(3)
                .enumerate()
                .filter_map(|(index, triangle)| {
                    let [a, b, c] = [triangle[0], triangle[1], triangle[2]];
                    (a != b && b != c && a != c).then(|| {
                        if index & 1 == 0 {
                            [u32::from(a), u32::from(b), u32::from(c)]
                        } else {
                            [u32::from(b), u32::from(a), u32::from(c)]
                        }
                    })
                })
        })
        .flatten()
        .collect()
}

pub(super) fn validate_triangle_index_stream(
    source: &[u32],
    vertex_count: usize,
    path: &str,
    family: NativeGeometrySourceFamily,
) -> Result<Vec<u32>, NativeGeometryLoweringError> {
    if source.is_empty() || source.len() % 3 != 0 {
        return Err(native_geometry_lowering_error(
            family,
            path,
            NativeGeometryLoweringErrorKind::InvalidTopology {
                detail: "index stream is empty or is not triangular",
            },
        ));
    }
    if source.iter().any(|index| *index as usize >= vertex_count) {
        return Err(native_geometry_lowering_error(
            family,
            path,
            NativeGeometryLoweringErrorKind::InvalidTopology {
                detail: "index references a vertex outside the geometry",
            },
        ));
    }
    Ok(source.to_vec())
}

pub(super) fn create_validated_vertex_stream(
    positions: impl Iterator<Item = [f32; 3]>,
    normals: impl Iterator<Item = Option<[f32; 3]>>,
    colors: impl Iterator<Item = Option<[f32; 4]>>,
    uvs: impl Iterator<Item = [Option<[f32; 3]>; 3]>,
    auxiliary_vectors: impl Iterator<Item = Option<[f32; 3]>>,
    indices: &[u32],
    path: &str,
    family: NativeGeometrySourceFamily,
) -> Result<Vec<VertexSource>, NativeGeometryLoweringError> {
    let positions = positions
        .map(convert_source_z_up_vector_to_bevy_y_up_coordinates)
        .collect::<Vec<_>>();
    if positions.is_empty() || positions.iter().flatten().any(|value| !value.is_finite()) {
        return Err(native_geometry_lowering_error(
            family,
            path,
            NativeGeometryLoweringErrorKind::InvalidVertexData {
                detail: "positions are empty or non-finite",
            },
        ));
    }
    let supplied_normals = normals
        .map(|normal| normal.map(convert_source_z_up_vector_to_bevy_y_up_coordinates))
        .collect::<Vec<_>>();
    let mut colors = colors.collect::<Vec<_>>();
    let authored_uvs = uvs.collect::<Vec<_>>();
    let uv_effects = authored_uvs
        .iter()
        .map(|sets| sets.map(|uv| uv.map_or(0.0, |uv| uv[2])))
        .collect::<Vec<_>>();
    let auxiliary_vectors = auxiliary_vectors
        .map(Option::unwrap_or_default)
        .collect::<Vec<_>>();
    let mut uvs = authored_uvs
        .into_iter()
        .map(|sets| sets.map(|uv| uv.map(|uv| [uv[0], uv[1]])))
        .collect::<Vec<_>>();
    if supplied_normals.len() != positions.len()
        || colors.len() != positions.len()
        || uvs.len() != positions.len()
        || uv_effects.len() != positions.len()
        || auxiliary_vectors.len() != positions.len()
    {
        return Err(native_geometry_lowering_error(
            family,
            path,
            NativeGeometryLoweringErrorKind::InvalidVertexData {
                detail: "vertex attribute streams have different lengths",
            },
        ));
    }
    // NIF declares optional attributes for a geometry block, not for an
    // individual vertex. Some shipped blocks contain partial or non-finite
    // optional streams. Such a stream was not usable by the original draw
    // either, so discard the whole optional channel rather than rejecting the
    // otherwise valid geometry or inventing per-vertex values.
    if !colors.iter().all(|color| {
        color.is_some_and(|color| color.into_iter().all(|component| component.is_finite()))
    }) {
        colors.fill(None);
    } else {
        colors.iter_mut().for_each(|color| {
            color
                .as_mut()
                .expect("the complete colour stream was checked")
                .iter_mut()
                .for_each(|component| *component = component.clamp(0.0, 1.0));
        });
    }
    for set in 0..3 {
        let retain_uv = uvs.iter().all(|sets| {
            sets[set].is_some_and(|uv| uv.into_iter().all(|component| component.is_finite()))
        });
        if !retain_uv {
            uvs.iter_mut().for_each(|sets| sets[set] = None);
        }
    }
    let derived_normals = derive_normals(&positions, indices);
    let normals = supplied_normals
        .into_iter()
        .enumerate()
        .map(|(index, normal)| normalized_or(normal, derived_normals[index]))
        .collect::<Vec<_>>();
    let tangent_uvs = uvs
        .iter()
        .map(|sets| sets[0].unwrap_or_default())
        .collect::<Vec<_>>();
    let tangents = derive_tangents(&positions, &normals, &tangent_uvs, indices);
    Ok(positions
        .into_iter()
        .zip(normals)
        .zip(colors)
        .zip(uvs)
        .zip(uv_effects)
        .zip(auxiliary_vectors)
        .zip(tangents)
        .map(
            |((((((position, normal), color), uvs), uv_effects), auxiliary_vector), tangent)| {
                VertexSource {
                    position,
                    normal,
                    tangent,
                    uvs,
                    uv_effects,
                    auxiliary_vector,
                    color,
                    joints: None,
                    weights: None,
                }
            },
        )
        .collect())
}

pub(super) fn native_geometry_lowering_error(
    family: NativeGeometrySourceFamily,
    path: &str,
    kind: NativeGeometryLoweringErrorKind,
) -> NativeGeometryLoweringError {
    NativeGeometryLoweringError::new(family, path, kind)
}
