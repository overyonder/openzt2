use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, Mesh, PrimitiveTopology},
    prelude::*,
};
use openzt2_game_data::terrain::{TerrainBiomeWaterPresentation, TerrainGrid, TerrainWaterfall};

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::{
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
    terrain_world_sampling::{sample_authored_terrain, sample_terrain},
};

pub(super) struct ConstructedTerrainWaterMeshGroup {
    pub(super) biome_index: u8,
    pub(super) surface_plane_height: f32,
    pub(super) horizontal_minimum: Vec2,
    pub(super) horizontal_maximum: Vec2,
    pub(super) maximum_absolute_input_height: f32,
    pub(super) surface_mesh: Mesh,
    pub(super) waterfall_mesh_groups: Vec<ConstructedTerrainWaterfallMeshGroup>,
}

pub(super) struct ConstructedTerrainWaterfallMeshGroup {
    pub(super) mesh: Mesh,
    pub(super) local_anchor_translation: Vec3,
}

struct TerrainWaterMeshBuffers {
    water_style: u16,
    biome_index: u8,
    surface_positions: Vec<[f32; 3]>,
    surface_normals: Vec<[f32; 3]>,
    surface_texture_coordinates: Vec<[f32; 2]>,
    surface_colours: Vec<[f32; 4]>,
    surface_indices: Vec<u32>,
    surface_plane_height: f32,
    horizontal_minimum: Vec2,
    horizontal_maximum: Vec2,
    maximum_absolute_input_height: f32,
    waterfall_strips: Vec<TerrainWaterfallStrip>,
}

#[cfg(test)]
mod water_source_world_position_tests {
    use super::*;

    #[test]
    fn translated_chunks_supply_world_positions_to_the_authored_shader() {
        let mut buffers = TerrainWaterMeshBuffers::new(0, 0, 3.0);
        buffers.add_horizontal_water_cell(
            [Vec2::ZERO, Vec2::X, Vec2::ONE, Vec2::Y],
            [3.0; 4],
            Vec2::new(32.0, 64.0),
            [1.0; 4],
        );
        assert_eq!(buffers.surface_positions[0], [32.0, 64.0, 3.0]);
        assert_eq!(buffers.surface_positions[2], [33.0, 65.0, 3.0]);
    }
}

#[derive(Clone, Copy)]
struct TerrainWaterfallStrip {
    edge: [Vec2; 2],
    top_heights: [f32; 2],
    bottom_height: f32,
    outward_normal: Vec3,
}

impl TerrainWaterMeshBuffers {
    fn new(water_style: u16, biome_index: u8, surface_plane_height: f32) -> Self {
        Self {
            water_style,
            biome_index,
            surface_positions: Vec::new(),
            surface_normals: Vec::new(),
            surface_texture_coordinates: Vec::new(),
            surface_colours: Vec::new(),
            surface_indices: Vec::new(),
            surface_plane_height,
            horizontal_minimum: Vec2::splat(f32::INFINITY),
            horizontal_maximum: Vec2::splat(f32::NEG_INFINITY),
            maximum_absolute_input_height: 0.0,
            waterfall_strips: Vec::new(),
        }
    }

    fn add_waterfall_edge(
        &mut self,
        edge: [Vec2; 2],
        top_heights: [f32; 2],
        bottom_height: f32,
        outward_normal: Vec3,
    ) {
        let (edge, top_heights) = if waterfall_edge_tangent_coordinate(edge[0], outward_normal)
            <= waterfall_edge_tangent_coordinate(edge[1], outward_normal)
        {
            (edge, top_heights)
        } else {
            ([edge[1], edge[0]], [top_heights[1], top_heights[0]])
        };
        self.waterfall_strips.push(TerrainWaterfallStrip {
            edge,
            top_heights,
            bottom_height,
            outward_normal,
        });
    }

    fn add_horizontal_water_cell(
        &mut self,
        local_corners: [Vec2; 4],
        surface_heights: [f32; 4],
        chunk_world_origin: Vec2,
        colour: [f32; 4],
    ) {
        let Ok(first_vertex_index) = u32::try_from(self.surface_positions.len()) else {
            return;
        };
        for (corner, surface_height) in local_corners.into_iter().zip(surface_heights) {
            self.horizontal_minimum = self.horizontal_minimum.min(chunk_world_origin + corner);
            self.horizontal_maximum = self.horizontal_maximum.max(chunk_world_origin + corner);
            self.maximum_absolute_input_height =
                self.maximum_absolute_input_height.max(surface_height.abs());
            // waterflat.fx evaluates waves, eye rays and fog in source world
            // space before applying WorldToNDC; no model matrix is consumed.
            let world_corner = chunk_world_origin + corner;
            self.surface_positions
                .push([world_corner.x, world_corner.y, surface_height]);
            self.surface_normals.push(Vec3::Z.to_array());
            self.surface_texture_coordinates
                .push((chunk_world_origin + corner).to_array());
            self.surface_colours.push(colour);
        }
        self.surface_indices.extend_from_slice(&[
            first_vertex_index,
            first_vertex_index + 2,
            first_vertex_index + 1,
            first_vertex_index,
            first_vertex_index + 3,
            first_vertex_index + 2,
        ]);
    }

    fn finish(
        self,
        waterfall_presentation: Option<&TerrainWaterfall>,
    ) -> ConstructedTerrainWaterMeshGroup {
        let surface_mesh = create_triangle_mesh(
            self.surface_positions,
            self.surface_normals,
            self.surface_texture_coordinates,
            Some(self.surface_colours),
            self.surface_indices,
        );
        let waterfall_mesh_groups = waterfall_presentation.map_or_else(Vec::new, |presentation| {
            construct_contiguous_terrain_waterfall_mesh_groups(self.waterfall_strips, presentation)
        });
        ConstructedTerrainWaterMeshGroup {
            biome_index: self.biome_index,
            surface_plane_height: self.surface_plane_height,
            horizontal_minimum: self.horizontal_minimum,
            horizontal_maximum: self.horizontal_maximum,
            maximum_absolute_input_height: self.maximum_absolute_input_height,
            surface_mesh,
            waterfall_mesh_groups,
        }
    }
}

pub(super) fn construct_terrain_water_surface_and_waterfall_mesh_groups(
    chunk: &TerrainChunk,
    terrain_asset: &TerrainAsset,
    edited_samples: Option<&EditedTerrainSamples>,
) -> Vec<ConstructedTerrainWaterMeshGroup> {
    let cell_count_per_side = usize::from(chunk.side.saturating_sub(1));
    let mut mesh_buffers_by_water_style = Vec::<TerrainWaterMeshBuffers>::new();
    for z in 0..cell_count_per_side {
        for x in 0..cell_count_per_side {
            let local_minimum = Vec2::new(x as f32, z as f32) * chunk.spacing_m;
            let local_maximum = local_minimum + Vec2::splat(chunk.spacing_m);
            let cell_center = chunk.origin + (local_minimum + local_maximum) * 0.5;
            let Some(authored_sample) =
                sample_authored_terrain(chunk, terrain_asset, edited_samples, cell_center)
            else {
                continue;
            };
            let (Some(water), Some(center_water_height)) = (
                authored_sample.water,
                authored_sample.geometry.water_height_m,
            ) else {
                continue;
            };
            let local_corners = [
                local_minimum,
                Vec2::new(local_maximum.x, local_minimum.y),
                local_maximum,
                Vec2::new(local_minimum.x, local_maximum.y),
            ];
            let corner_samples = local_corners.map(|corner| {
                sample_terrain(chunk, terrain_asset, edited_samples, chunk.origin + corner)
            });
            let surface_heights = corner_samples.map(|corner_sample| {
                corner_sample
                    .and_then(|sample| sample.water_height_m)
                    .unwrap_or(center_water_height)
            });
            let mesh_buffer_index = mesh_buffers_by_water_style
                .iter()
                .position(|mesh_buffers| {
                    mesh_buffers.water_style == water.style
                        && mesh_buffers.biome_index == water.biome_channel
                        && mesh_buffers.surface_plane_height.to_bits()
                            == center_water_height.to_bits()
                })
                .unwrap_or_else(|| {
                    mesh_buffers_by_water_style.push(TerrainWaterMeshBuffers::new(
                        water.style,
                        water.biome_channel,
                        center_water_height,
                    ));
                    mesh_buffers_by_water_style.len() - 1
                });
            let water_depth_ratio = terrain_water_depth_ratio_for_cell(
                terrain_asset.canonical_terrain_grid(),
                chunk,
                x,
                z,
                water.style,
            );
            let surface_colour = terrain_asset
                .canonical_terrain_grid()
                .biomes
                .get(usize::from(water.biome_channel))
                .and_then(|biome| biome.water_presentation.as_ref())
                .map_or([1.0; 4], |presentation| {
                    interpolate_authored_terrain_water_vertex_colour(
                        presentation,
                        water_depth_ratio,
                    )
                });
            mesh_buffers_by_water_style[mesh_buffer_index].add_horizontal_water_cell(
                local_corners,
                surface_heights,
                chunk.origin,
                surface_colour,
            );
            for (neighbour_offset, edge, outward_normal) in [
                (
                    IVec2::new(-1, 0),
                    [local_corners[3], local_corners[0]],
                    Vec3::NEG_X,
                ),
                (
                    IVec2::new(1, 0),
                    [local_corners[1], local_corners[2]],
                    Vec3::X,
                ),
                (
                    IVec2::new(0, -1),
                    [local_corners[0], local_corners[1]],
                    Vec3::NEG_Z,
                ),
                (
                    IVec2::new(0, 1),
                    [local_corners[2], local_corners[3]],
                    Vec3::Z,
                ),
            ] {
                let neighbour_cell = IVec2::new(x as i32, z as i32) + neighbour_offset;
                if neighbour_cell.x < 0
                    || neighbour_cell.y < 0
                    || neighbour_cell.x >= cell_count_per_side as i32
                    || neighbour_cell.y >= cell_count_per_side as i32
                {
                    continue;
                }
                let neighbour_center =
                    chunk.origin + (neighbour_cell.as_vec2() + Vec2::splat(0.5)) * chunk.spacing_m;
                let Some(neighbour_sample) =
                    sample_terrain(chunk, terrain_asset, edited_samples, neighbour_center)
                else {
                    continue;
                };
                let Some(bottom_height) = neighbour_sample.water_height_m else {
                    continue;
                };
                if center_water_height <= bottom_height + 0.01 {
                    continue;
                }
                let Some(waterfall_presentation) =
                    terrain_asset.canonical_terrain_grid().waterfall.as_ref()
                else {
                    continue;
                };
                let top_heights = edge.map(|corner| {
                    sample_terrain(chunk, terrain_asset, edited_samples, chunk.origin + corner)
                        .map(|sample| {
                            if waterfall_presentation.float_on_water {
                                sample.water_height_m.unwrap_or(center_water_height)
                            } else {
                                sample.height_m
                            }
                        })
                        .unwrap_or(center_water_height)
                });
                mesh_buffers_by_water_style[mesh_buffer_index].add_waterfall_edge(
                    edge,
                    top_heights,
                    bottom_height,
                    outward_normal,
                );
            }
        }
    }
    mesh_buffers_by_water_style
        .into_iter()
        .filter(|mesh_buffers| !mesh_buffers.surface_indices.is_empty())
        .map(|mesh_buffers| {
            mesh_buffers.finish(terrain_asset.canonical_terrain_grid().waterfall.as_ref())
        })
        .collect()
}

fn terrain_water_depth_ratio_for_cell(
    terrain_grid: &TerrainGrid,
    chunk: &TerrainChunk,
    local_x: usize,
    local_z: usize,
    water_style: u16,
) -> f32 {
    let cells_per_chunk = i64::from(chunk.side.saturating_sub(1));
    let Some(global_x) = i64::from(chunk.source_coord.x)
        .checked_mul(cells_per_chunk)
        .and_then(|origin| i64::try_from(local_x).ok()?.checked_add(origin))
    else {
        return if water_style <= 1 { 0.0 } else { 1.0 };
    };
    let Some(global_z) = i64::from(chunk.source_coord.y)
        .checked_mul(cells_per_chunk)
        .and_then(|origin| {
            usize::try_from(cells_per_chunk)
                .ok()?
                .checked_sub(local_z + 1)
                .and_then(|source_local_z| i64::try_from(source_local_z).ok())
                .and_then(|source_local_z| origin.checked_add(source_local_z))
        })
    else {
        return if water_style <= 1 { 0.0 } else { 1.0 };
    };
    let row_major_cell_index = (global_x >= 0 && global_z >= 0)
        .then(|| {
            u32::try_from(global_z)
                .ok()?
                .checked_mul(terrain_grid.width)?
                .checked_add(u32::try_from(global_x).ok()?)
        })
        .flatten();
    row_major_cell_index
        .and_then(|cell_index| {
            terrain_grid
                .water_regions
                .iter()
                .find(|region| region.row_major_cell_indices.contains(&cell_index))
                .map(|region| region.depth_ratio)
        })
        .unwrap_or_else(|| if water_style <= 1 { 0.0 } else { 1.0 })
        .clamp(0.0, 1.0)
}

fn interpolate_authored_terrain_water_vertex_colour(
    presentation: &TerrainBiomeWaterPresentation,
    water_depth_ratio: f32,
) -> [f32; 4] {
    let normalize = |colour: [u8; 4]| colour.map(|channel| f32::from(channel) / 255.0);
    let low = Vec4::from_array(normalize(presentation.vertex_colour_low));
    let medium = Vec4::from_array(normalize(presentation.vertex_colour_medium));
    let high = Vec4::from_array(normalize(presentation.vertex_colour_high));
    if water_depth_ratio <= 0.5 {
        low.lerp(medium, water_depth_ratio * 2.0).to_array()
    } else {
        medium
            .lerp(high, (water_depth_ratio - 0.5) * 2.0)
            .to_array()
    }
}

fn construct_contiguous_terrain_waterfall_mesh_groups(
    mut strips: Vec<TerrainWaterfallStrip>,
    presentation: &TerrainWaterfall,
) -> Vec<ConstructedTerrainWaterfallMeshGroup> {
    strips.sort_unstable_by(|left, right| {
        waterfall_facing_index(left.outward_normal)
            .cmp(&waterfall_facing_index(right.outward_normal))
            .then_with(|| {
                waterfall_edge_line_coordinate(left.edge[0], left.outward_normal).total_cmp(
                    &waterfall_edge_line_coordinate(right.edge[0], right.outward_normal),
                )
            })
            .then_with(|| {
                waterfall_edge_tangent_coordinate(left.edge[0], left.outward_normal).total_cmp(
                    &waterfall_edge_tangent_coordinate(right.edge[0], right.outward_normal),
                )
            })
    });
    let mut result = Vec::new();
    let mut group_start = 0;
    while group_start < strips.len() {
        let mut group_end = group_start + 1;
        while group_end < strips.len()
            && group_end - group_start < 25
            && terrain_waterfall_strips_are_contiguous(strips[group_end - 1], strips[group_end])
        {
            group_end += 1;
        }
        result.push(construct_terrain_waterfall_mesh_group(
            &strips[group_start..group_end],
            presentation,
        ));
        group_start = group_end;
    }
    result
}

fn terrain_waterfall_strips_are_contiguous(
    first: TerrainWaterfallStrip,
    second: TerrainWaterfallStrip,
) -> bool {
    waterfall_facing_index(first.outward_normal) == waterfall_facing_index(second.outward_normal)
        && (waterfall_edge_line_coordinate(first.edge[0], first.outward_normal)
            - waterfall_edge_line_coordinate(second.edge[0], second.outward_normal))
        .abs()
            <= 0.01
        && first.edge[1].distance(second.edge[0]) <= 0.01
}

fn construct_terrain_waterfall_mesh_group(
    strips: &[TerrainWaterfallStrip],
    presentation: &TerrainWaterfall,
) -> ConstructedTerrainWaterfallMeshGroup {
    let first = strips[0];
    let last = strips[strips.len() - 1];
    let anchor_edge_point = (first.edge[0] + last.edge[1]) * 0.5;
    let anchor_height =
        (first.top_heights[0] + last.top_heights[1]) * 0.5 + presentation.height_offset_metres;
    let anchor = Vec3::new(anchor_edge_point.x, anchor_height, anchor_edge_point.y);
    let total_width = strips
        .iter()
        .map(|strip| strip.edge[0].distance(strip.edge[1]))
        .sum::<f32>()
        .max(f32::EPSILON);
    let mut positions = Vec::with_capacity(strips.len() * 4);
    let mut normals = Vec::with_capacity(strips.len() * 4);
    let mut texture_coordinates = Vec::with_capacity(strips.len() * 4);
    let mut detail_texture_coordinates = Vec::with_capacity(strips.len() * 4);
    let mut colours = Vec::with_capacity(strips.len() * 4);
    let mut indices = Vec::with_capacity(strips.len() * 6);
    let mut accumulated_width = 0.0;
    for strip in strips {
        let Ok(first_vertex_index) = u32::try_from(positions.len()) else {
            break;
        };
        let edge_width = strip.edge[0].distance(strip.edge[1]);
        let first_u = accumulated_width / total_width;
        accumulated_width += edge_width;
        let second_u = accumulated_width / total_width;
        let edge_height =
            (strip.top_heights[0].max(strip.top_heights[1]) - strip.bottom_height).max(0.0);
        let texture_height = edge_height / presentation.decal_height_metres.max(f32::EPSILON);
        positions.extend_from_slice(&[
            (Vec3::new(
                strip.edge[0].x,
                strip.top_heights[0] + presentation.height_offset_metres,
                strip.edge[0].y,
            ) - anchor)
                .to_array(),
            (Vec3::new(
                strip.edge[1].x,
                strip.top_heights[1] + presentation.height_offset_metres,
                strip.edge[1].y,
            ) - anchor)
                .to_array(),
            (Vec3::new(
                strip.edge[1].x,
                strip.bottom_height + presentation.height_offset_metres,
                strip.edge[1].y,
            ) - anchor)
                .to_array(),
            (Vec3::new(
                strip.edge[0].x,
                strip.bottom_height + presentation.height_offset_metres,
                strip.edge[0].y,
            ) - anchor)
                .to_array(),
        ]);
        normals.extend_from_slice(&[strip.outward_normal.to_array(); 4]);
        let strip_texture_coordinates = [
            [first_u, 0.0],
            [second_u, 0.0],
            [second_u, texture_height],
            [first_u, texture_height],
        ];
        texture_coordinates.extend_from_slice(&strip_texture_coordinates);
        detail_texture_coordinates.extend_from_slice(&strip_texture_coordinates);
        colours.extend_from_slice(&[[1.0; 4]; 4]);
        indices.extend_from_slice(&[
            first_vertex_index,
            first_vertex_index + 1,
            first_vertex_index + 2,
            first_vertex_index,
            first_vertex_index + 2,
            first_vertex_index + 3,
        ]);
    }
    let mut mesh = create_triangle_mesh(
        positions,
        normals,
        texture_coordinates,
        Some(colours),
        indices,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, detail_texture_coordinates);
    ConstructedTerrainWaterfallMeshGroup {
        mesh,
        local_anchor_translation: anchor,
    }
}

fn waterfall_facing_index(outward_normal: Vec3) -> u8 {
    if outward_normal.x < -0.5 {
        0
    } else if outward_normal.x > 0.5 {
        1
    } else if outward_normal.z < -0.5 {
        2
    } else {
        3
    }
}

fn waterfall_edge_line_coordinate(point: Vec2, outward_normal: Vec3) -> f32 {
    if outward_normal.x.abs() > 0.5 {
        point.x
    } else {
        point.y
    }
}

fn waterfall_edge_tangent_coordinate(point: Vec2, outward_normal: Vec3) -> f32 {
    if outward_normal.x.abs() > 0.5 {
        point.y
    } else {
        point.x
    }
}

fn create_triangle_mesh(
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    texture_coordinates: Vec<[f32; 2]>,
    colours: Option<Vec<[f32; 4]>>,
    indices: Vec<u32>,
) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, texture_coordinates);
    if let Some(colours) = colours {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colours);
    }
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
