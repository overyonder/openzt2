use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::{
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
    terrain_sample_grid_queries::{
        terrain_cell_uses_alternate_triangle_diagonal, terrain_sample_height_metres,
    },
};

/// Constructs a mesh from the terrain triangles beneath a square world-space footprint.
#[allow(clippy::many_single_char_names)] // Compact cell-corner names expose the authored diagonal directly.
pub(crate) fn construct_square_surface_mesh_fitted_to_canonical_terrain_samples(
    width: f32,
    centre: Vec3,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) -> Option<Mesh> {
    construct_square_surface_mesh_fitted_to_canonical_terrain_samples_with_rotated_texture_coordinates(
        width,
        centre,
        0.0,
        terrain_index,
        terrain_assets,
        terrain_chunks,
    )
}

pub(crate) fn construct_square_surface_mesh_fitted_to_canonical_terrain_samples_with_rotated_texture_coordinates(
    width: f32,
    centre: Vec3,
    texture_rotation_radians: f32,
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
) -> Option<Mesh> {
    if !width.is_finite()
        || width <= 0.0
        || !centre.is_finite()
        || !terrain_index.chunk_span_m.is_finite()
        || terrain_index.chunk_span_m <= 0.0
    {
        return None;
    }

    let half_width = width * 0.5;
    let footprint_minimum = centre.xz() - Vec2::splat(half_width);
    let footprint_maximum = centre.xz() + Vec2::splat(half_width);
    let minimum_chunk_coordinate = ((footprint_minimum - terrain_index.origin)
        / terrain_index.chunk_span_m)
        .floor()
        .as_ivec2();
    let maximum_chunk_coordinate = ((footprint_maximum - terrain_index.origin)
        / terrain_index.chunk_span_m)
        .floor()
        .as_ivec2();

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for chunk_z in minimum_chunk_coordinate.y..=maximum_chunk_coordinate.y {
        for chunk_x in minimum_chunk_coordinate.x..=maximum_chunk_coordinate.x {
            let chunk_entity = terrain_index.chunks.get(&IVec2::new(chunk_x, chunk_z))?;
            let (chunk, edited) = terrain_chunks.get(*chunk_entity).ok()?;
            let asset = terrain_assets.get(&chunk.asset)?;
            let cell_count = usize::from(chunk.side.checked_sub(1)?);
            if !chunk.spacing_m.is_finite() || chunk.spacing_m <= 0.0 || cell_count == 0 {
                return None;
            }
            let local_minimum = ((footprint_minimum - chunk.origin) / chunk.spacing_m).floor();
            let local_maximum = ((footprint_maximum - chunk.origin) / chunk.spacing_m).floor();
            let minimum_cell_x = local_minimum.x.max(0.0) as usize;
            let minimum_cell_z = local_minimum.y.max(0.0) as usize;
            let maximum_cell_x = (local_maximum.x.max(0.0) as usize).min(cell_count - 1);
            let maximum_cell_z = (local_maximum.y.max(0.0) as usize).min(cell_count - 1);
            if minimum_cell_x > maximum_cell_x || minimum_cell_z > maximum_cell_z {
                continue;
            }

            for z in minimum_cell_z..=maximum_cell_z {
                for x in minimum_cell_x..=maximum_cell_x {
                    let cell_origin =
                        chunk.origin + Vec2::new(x as f32, z as f32) * chunk.spacing_m;
                    let a = Vec3::new(
                        cell_origin.x,
                        terrain_sample_height_metres(chunk, asset, edited, x, z)?,
                        cell_origin.y,
                    );
                    let b = Vec3::new(
                        cell_origin.x + chunk.spacing_m,
                        terrain_sample_height_metres(chunk, asset, edited, x + 1, z)?,
                        cell_origin.y,
                    );
                    let c = Vec3::new(
                        cell_origin.x,
                        terrain_sample_height_metres(chunk, asset, edited, x, z + 1)?,
                        cell_origin.y + chunk.spacing_m,
                    );
                    let d = Vec3::new(
                        cell_origin.x + chunk.spacing_m,
                        terrain_sample_height_metres(chunk, asset, edited, x + 1, z + 1)?,
                        cell_origin.y + chunk.spacing_m,
                    );
                    let triangles = if terrain_cell_uses_alternate_triangle_diagonal(
                        chunk,
                        asset,
                        UVec2::new(x as u32, z as u32),
                    )? {
                        [[a, c, b], [b, c, d]]
                    } else {
                        [[a, c, d], [a, d, b]]
                    };
                    for triangle in triangles {
                        append_canonical_terrain_triangle_clipped_to_square_footprint(
                            triangle,
                            footprint_minimum,
                            footprint_maximum,
                            centre,
                            width,
                            texture_rotation_radians,
                            &mut positions,
                            &mut normals,
                            &mut uvs,
                            &mut indices,
                        )?;
                    }
                }
            }
        }
    }
    if positions.is_empty() {
        return None;
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

#[allow(clippy::too_many_arguments)]
fn append_canonical_terrain_triangle_clipped_to_square_footprint(
    triangle: [Vec3; 3],
    footprint_minimum: Vec2,
    footprint_maximum: Vec2,
    centre: Vec3,
    width: f32,
    texture_rotation_radians: f32,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) -> Option<()> {
    let mut clipped_vertices = [Vec3::ZERO; 8];
    clipped_vertices[..3].copy_from_slice(&triangle);
    let mut clipped_vertex_count = 3;
    let mut clipping_scratch = [Vec3::ZERO; 8];
    for (axis, boundary, retain_greater_coordinate) in [
        (0, footprint_minimum.x, true),
        (0, footprint_maximum.x, false),
        (2, footprint_minimum.y, true),
        (2, footprint_maximum.y, false),
    ] {
        clipped_vertex_count = clip_terrain_triangle_polygon_against_footprint_boundary(
            &clipped_vertices[..clipped_vertex_count],
            &mut clipping_scratch,
            axis,
            boundary,
            retain_greater_coordinate,
        );
        if clipped_vertex_count < 3 {
            return Some(());
        }
        core::mem::swap(&mut clipped_vertices, &mut clipping_scratch);
    }

    let normal = (triangle[1] - triangle[0])
        .cross(triangle[2] - triangle[0])
        .normalize_or(Vec3::Y)
        .to_array();
    let first_vertex_index = u32::try_from(positions.len()).ok()?;
    for vertex in &clipped_vertices[..clipped_vertex_count] {
        positions.push([
            vertex.x - centre.x,
            vertex.y - centre.y + 0.015,
            vertex.z - centre.z,
        ]);
        normals.push(normal);
        let unrotated_texture_coordinate = Vec2::new(
            (vertex.x - footprint_minimum.x) / width,
            (vertex.z - footprint_minimum.y) / width,
        );
        let texture_coordinate = Vec2::splat(0.5)
            + Mat2::from_angle(texture_rotation_radians)
                * (unrotated_texture_coordinate - Vec2::splat(0.5));
        uvs.push(texture_coordinate.to_array());
    }
    for triangle_index in 1..clipped_vertex_count - 1 {
        indices.extend_from_slice(&[
            first_vertex_index,
            first_vertex_index.checked_add(u32::try_from(triangle_index).ok()?)?,
            first_vertex_index.checked_add(u32::try_from(triangle_index + 1).ok()?)?,
        ]);
    }
    Some(())
}

fn clip_terrain_triangle_polygon_against_footprint_boundary(
    vertices: &[Vec3],
    result: &mut [Vec3; 8],
    axis: usize,
    boundary: f32,
    retain_greater_coordinate: bool,
) -> usize {
    let Some(mut previous) = vertices.last().copied() else {
        return 0;
    };
    let mut previous_inside = terrain_triangle_vertex_is_inside_footprint_boundary(
        previous,
        axis,
        boundary,
        retain_greater_coordinate,
    );
    let mut result_length = 0;
    for current in vertices.iter().copied() {
        let current_inside = terrain_triangle_vertex_is_inside_footprint_boundary(
            current,
            axis,
            boundary,
            retain_greater_coordinate,
        );
        if current_inside != previous_inside {
            let coordinate_span = current[axis] - previous[axis];
            if coordinate_span.abs() > f32::EPSILON {
                let interpolation = (boundary - previous[axis]) / coordinate_span;
                result[result_length] = previous.lerp(current, interpolation);
                result_length += 1;
            }
        }
        if current_inside {
            result[result_length] = current;
            result_length += 1;
        }
        previous = current;
        previous_inside = current_inside;
    }
    result_length
}

fn terrain_triangle_vertex_is_inside_footprint_boundary(
    vertex: Vec3,
    axis: usize,
    boundary: f32,
    retain_greater_coordinate: bool,
) -> bool {
    if retain_greater_coordinate {
        vertex[axis] >= boundary
    } else {
        vertex[axis] <= boundary
    }
}
