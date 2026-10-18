use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::Mesh;
use openzt2_zoo_state::terrain::terrain_water_shoreline_geometry_types::TerrainShorelinePatch;

use super::patch_deformation::deform_shoreline_patch;
use super::ConstructedTerrainShorelineMeshGroup;

const PATCHES_PER_MESH: u16 = 1500 / 16;

// Each side owns eight triangles of the five-by-five patch.
const SIDE_INDICES: [[u32; 24]; 4] = [
    [
        16, 12, 17, 12, 18, 17, 20, 16, 21, 16, 17, 21, 21, 17, 22, 17, 18, 22, 22, 18, 23, 18, 24,
        23,
    ],
    [
        4, 9, 8, 12, 8, 13, 8, 9, 13, 13, 9, 14, 12, 13, 18, 13, 14, 18, 18, 14, 19, 18, 19, 24,
    ],
    [
        0, 1, 6, 1, 2, 6, 6, 2, 7, 2, 3, 7, 7, 3, 8, 3, 4, 8, 6, 7, 12, 7, 8, 12,
    ],
    [
        0, 6, 5, 5, 6, 10, 10, 6, 11, 6, 12, 11, 10, 11, 15, 15, 11, 16, 11, 12, 16, 15, 16, 20,
    ],
];

pub(super) fn construct_shoreline_mesh_groups(
    patches: impl IntoIterator<Item = TerrainShorelinePatch>,
    biome_index: u8,
    surface_plane_height: f32,
) -> Vec<ConstructedTerrainShorelineMeshGroup> {
    let mut groups = Vec::new();
    let mut positions = Vec::with_capacity(usize::from(PATCHES_PER_MESH) * 25);
    let mut texture_coordinates = Vec::with_capacity(usize::from(PATCHES_PER_MESH) * 25);
    let mut indices = Vec::with_capacity(usize::from(PATCHES_PER_MESH) * 96);
    let mut patch_count = 0;
    for mut patch in patches {
        if !patch.included_sides.into_iter().any(|included| included) {
            continue;
        }
        let base = u32::from(patch_count) * 25;
        append_deformed_patch_indices(&mut patch, base, &mut indices);
        positions.extend(patch.positions);
        texture_coordinates.extend(patch.texture_coordinates);
        patch_count += 1;
        if patch_count == PATCHES_PER_MESH {
            groups.push(finish_mesh(
                std::mem::replace(
                    &mut positions,
                    Vec::with_capacity(usize::from(PATCHES_PER_MESH) * 25),
                ),
                std::mem::replace(
                    &mut texture_coordinates,
                    Vec::with_capacity(usize::from(PATCHES_PER_MESH) * 25),
                ),
                std::mem::replace(
                    &mut indices,
                    Vec::with_capacity(usize::from(PATCHES_PER_MESH) * 96),
                ),
                biome_index,
                surface_plane_height,
            ));
            patch_count = 0;
        }
    }
    if !indices.is_empty() {
        groups.push(finish_mesh(
            positions,
            texture_coordinates,
            indices,
            biome_index,
            surface_plane_height,
        ));
    }
    groups
}

pub(super) fn append_deformed_patch_indices(
    patch: &mut TerrainShorelinePatch,
    base: u32,
    indices: &mut Vec<u32>,
) {
    if patch.included_sides.into_iter().all(|included| included) {
        for y in 0..4 {
            for x in 0..4 {
                let corner = y * 5 + x;
                indices.extend(
                    [
                        corner + 6,
                        corner + 5,
                        corner,
                        corner,
                        corner + 1,
                        corner + 6,
                    ]
                    .map(|index| base + index),
                );
            }
        }
    } else {
        for (side, included) in patch.included_sides.into_iter().enumerate() {
            if included {
                indices.extend(SIDE_INDICES[side].map(|index| base + index));
            }
        }
    }
    for (side, included) in patch.included_sides.into_iter().enumerate() {
        if !included {
            continue;
        }
        let code = patch.selector_codes[side];
        let neighbour = match code {
            5 | 9 | 13 | 16 => Some((side + 3) % 4),
            6 | 10 | 20 | 23 => Some((side + 1) % 4),
            _ => None,
        };
        if neighbour.is_some_and(|neighbour| patch.included_sides[neighbour]) {
            continue;
        }
        deform_shoreline_patch(&mut patch.positions, side, code);
    }
}

fn finish_mesh(
    positions: Vec<[f32; 3]>,
    texture_coordinates: Vec<[f32; 2]>,
    indices: Vec<u32>,
    biome_index: u8,
    surface_plane_height: f32,
) -> ConstructedTerrainShorelineMeshGroup {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, texture_coordinates);
    mesh.insert_indices(Indices::U32(indices));
    ConstructedTerrainShorelineMeshGroup {
        mesh,
        biome_index,
        surface_plane_height,
    }
}
