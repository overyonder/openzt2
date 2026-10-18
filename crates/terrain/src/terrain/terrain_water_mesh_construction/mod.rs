use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::mesh::Mesh;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use openzt2_game_data::terrain::TerrainWaterfall;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::terrain_water_shoreline_geometry::stitch_geometry::construct_terrain_water_geometric_wake_tile;
use super::terrain_water_shoreline_geometry::stitch_geometry::construct_terrain_water_stitch_tile;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionView,
};
use openzt2_zoo_state::terrain::terrain_water_shoreline_geometry_types::TerrainWaterStitchGeometry;
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain;

pub(super) struct ConstructedTerrainWaterMeshGroup {
    pub(super) region_owner: Entity,
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

pub(super) struct ConstructedTerrainWaterLowDetailMeshGroup {
    pub(super) region_owner: Entity,
    pub(super) mesh: Mesh,
}

/// Both fixed-function detail levels retain every authored texture coordinate
/// set; their material selects the active texture layers.
pub(super) fn construct_terrain_water_low_detail_mesh_groups(
    chunk: &TerrainChunk,
    terrain_asset: &TerrainAsset,
    _edited_samples: Option<&EditedTerrainSamples>,
    membership: &TerrainWaterMembershipIndex,
    sources: &[(Entity, TerrainWaterRegionView<'_>)],
) -> Option<Vec<ConstructedTerrainWaterLowDetailMeshGroup>> {
    if membership.terrain() != chunk.asset.id() {
        return None;
    }
    let cells_per_chunk = u32::from(terrain_asset.terrain_chunk_cells_per_side()?);
    if cells_per_chunk == 0 || cells_per_chunk % 4 != 0 {
        return None;
    }
    let mut groups = Vec::new();
    for (owner, source) in sources {
        let owner = *owner;
        if source.terrain().id() != chunk.asset.id() {
            continue;
        }
        let presentation = source.biome(terrain_asset)?.water_presentation.as_ref()?;
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        for tile in membership.render_tiles(owner) {
            if (tile * 4 / cells_per_chunk).as_ivec2() != chunk.source_coord {
                continue;
            }
            let patch = construct_terrain_water_stitch_tile(
                terrain_asset,
                membership,
                owner,
                source,
                tile,
            )?;
            let base = u32::try_from(positions.len()).ok()?;
            positions.extend(patch.positions);
            indices.extend(patch.indices.into_iter().map(|index| base + index));
        }
        if indices.is_empty() {
            continue;
        }
        let uvs = presentation.texture_animations.each_ref().map(|animation| {
            let scale = Vec2::from_array(animation.grid_dimensions) * 3.0;
            positions
                .iter()
                .map(|position: &[f32; 3]| (Vec2::new(position[0], position[1]) / scale).to_array())
                .collect::<Vec<_>>()
        });
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            vec![Vec3::Z.to_array(); positions.len()],
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        let [uv0, uv1] = uvs;
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv0);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, uv1);
        mesh.insert_indices(Indices::U32(indices));
        groups.push(ConstructedTerrainWaterLowDetailMeshGroup {
            region_owner: owner,
            mesh,
        });
    }
    Some(groups)
}

struct TerrainWaterMeshBuffers {
    region_owner: Entity,
    surface_positions: Vec<[f32; 3]>,
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
    fn surface_patch_masks_remain_independent_of_plane_height() {
        let mut buffers = TerrainWaterMeshBuffers::new(Entity::PLACEHOLDER, 3.0);
        assert!(buffers
            .add_horizontal_water_patch(TerrainWaterStitchGeometry {
                positions: vec![[32.0, 64.0, 0.0], [33.0, 64.0, 1.0], [33.0, 65.0, 1.0]],
                indices: vec![0, 1, 2],
            })
            .is_some());
        assert_eq!(buffers.surface_positions[0], [32.0, 64.0, 0.0]);
        assert_eq!(buffers.surface_positions[2], [33.0, 65.0, 1.0]);
        assert_eq!(buffers.maximum_absolute_input_height, 1.0);
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
    fn new(region_owner: Entity, surface_plane_height: f32) -> Self {
        Self {
            region_owner,
            surface_positions: Vec::new(),
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

    fn add_horizontal_water_patch(&mut self, patch: TerrainWaterStitchGeometry) -> Option<()> {
        let first_vertex_index = u32::try_from(self.surface_positions.len()).ok()?;
        for position in &patch.positions {
            let world_corner = Vec2::new(position[0], -position[1]);
            self.horizontal_minimum = self.horizontal_minimum.min(world_corner);
            self.horizontal_maximum = self.horizontal_maximum.max(world_corner);
            self.maximum_absolute_input_height =
                self.maximum_absolute_input_height.max(position[2].abs());
        }
        self.surface_positions.extend(patch.positions);
        self.surface_indices.extend(
            patch
                .indices
                .into_iter()
                .map(|index| first_vertex_index + index),
        );
        Some(())
    }

    fn finish(
        self,
        waterfall_presentation: Option<&TerrainWaterfall>,
    ) -> ConstructedTerrainWaterMeshGroup {
        let mut surface_mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        surface_mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.surface_positions);
        surface_mesh.insert_indices(Indices::U32(self.surface_indices));
        let waterfall_mesh_groups = waterfall_presentation.map_or_else(Vec::new, |presentation| {
            construct_contiguous_terrain_waterfall_mesh_groups(self.waterfall_strips, presentation)
        });
        ConstructedTerrainWaterMeshGroup {
            region_owner: self.region_owner,
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
    membership: &TerrainWaterMembershipIndex,
    sources: &[(Entity, TerrainWaterRegionView<'_>)],
) -> Option<Vec<ConstructedTerrainWaterMeshGroup>> {
    if membership.terrain() != chunk.asset.id() {
        return None;
    }
    let cell_count_per_side = usize::from(chunk.side.saturating_sub(1));
    let mut mesh_buffers_by_region = Vec::<TerrainWaterMeshBuffers>::new();
    let cells_per_chunk = u32::from(terrain_asset.terrain_chunk_cells_per_side()?);
    if cells_per_chunk == 0 || cells_per_chunk % 4 != 0 {
        return None;
    }
    for (owner, source) in sources {
        let owner = *owner;
        if source.terrain().id() != chunk.asset.id() {
            continue;
        }
        let mut buffers =
            TerrainWaterMeshBuffers::new(owner, source.surface_height(terrain_asset)?);
        for tile in membership.render_tiles(owner) {
            if (tile * 4 / cells_per_chunk).as_ivec2() != chunk.source_coord {
                continue;
            }
            let patch = construct_terrain_water_geometric_wake_tile(
                terrain_asset,
                membership,
                owner,
                source,
                tile,
            )?;
            buffers.add_horizontal_water_patch(patch)?;
        }
        if !buffers.surface_indices.is_empty() {
            mesh_buffers_by_region.push(buffers);
        }
    }
    for z in 0..cell_count_per_side {
        for x in 0..cell_count_per_side {
            let local_minimum = Vec2::new(x as f32, z as f32) * chunk.spacing_m;
            let local_maximum = local_minimum + Vec2::splat(chunk.spacing_m);
            let cell_center = chunk.origin + (local_minimum + local_maximum) * 0.5;
            let Some(Some(region_owner)) = membership.owner_at_world_xz(cell_center, terrain_asset)
            else {
                continue;
            };
            let (_, source) = sources.iter().find(|(owner, source)| {
                *owner == region_owner && source.terrain().id() == chunk.asset.id()
            })?;
            let center_water_height = source.surface_height(terrain_asset)?;
            let local_corners = [
                local_minimum,
                Vec2::new(local_maximum.x, local_minimum.y),
                local_maximum,
                Vec2::new(local_minimum.x, local_maximum.y),
            ];
            let Some(mesh_buffer_index) = mesh_buffers_by_region
                .iter()
                .position(|mesh_buffers| mesh_buffers.region_owner == region_owner)
            else {
                continue;
            };
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
                let Some(Some(neighbour_owner)) =
                    membership.owner_at_world_xz(neighbour_center, terrain_asset)
                else {
                    continue;
                };
                let (_, neighbour_source) = sources.iter().find(|(owner, source)| {
                    *owner == neighbour_owner && source.terrain().id() == chunk.asset.id()
                })?;
                let bottom_height = neighbour_source.surface_height(terrain_asset)?;
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
                mesh_buffers_by_region[mesh_buffer_index].add_waterfall_edge(
                    edge,
                    top_heights,
                    bottom_height,
                    outward_normal,
                );
            }
        }
    }
    Some(
        mesh_buffers_by_region
            .into_iter()
            .filter(|mesh_buffers| !mesh_buffers.surface_indices.is_empty())
            .map(|mesh_buffers| {
                mesh_buffers.finish(terrain_asset.canonical_terrain_grid().waterfall.as_ref())
            })
            .collect(),
    )
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
