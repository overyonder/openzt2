pub(super) mod presentation;
pub(super) mod systems;
pub(super) mod view_occlusion;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

/// The ray volume uses a closed angular seam and an open vertical axis.
pub(super) fn build_water_ray_strip_mesh() -> Mesh {
    let mut positions = Vec::with_capacity(121);
    let mut normals = Vec::with_capacity(121);
    let mut colours = Vec::with_capacity(121);
    let mut uvs = Vec::with_capacity(121);
    for row in 0..=10 {
        let height = row as f32 * 20.0;
        let fade = if height < 40.0 {
            (height / 40.0).sqrt()
        } else {
            ((height - 200.0) / (40.0 - 200.0)).powi(2)
        };
        for column in 0..=10 {
            let angle = column as f32 * (std::f32::consts::TAU / 10.0);
            // Copy the first vertex at the seam, avoiding a trigonometric gap.
            let (sine, cosine) = if column == 10 {
                (0.0, 1.0)
            } else {
                angle.sin_cos()
            };
            positions.push([10.0 * cosine, 10.0 * sine, height - 6.0]);
            normals.push([cosine, sine, 0.0]);
            colours.push([quantized_vertex_fade(fade); 4]);
            uvs.push([column as f32 / 10.0, height / 200.0]);
        }
    }
    let mut indices = Vec::with_capacity(600);
    for row in 0..10 {
        for column in 0..10 {
            let first = row * 11 + column;
            let next = first + 11;
            indices.extend_from_slice(&[first, first + 1, next, next, first + 1, next + 1]);
        }
    }
    water_ray_mesh(positions, normals, colours, uvs, indices)
}

pub(super) fn build_water_ray_disk_mesh() -> Mesh {
    let mut positions = Vec::with_capacity(81);
    let mut normals = Vec::with_capacity(81);
    let mut colours = Vec::with_capacity(81);
    let mut uvs = Vec::with_capacity(81);
    positions.push([0.0; 3]);
    normals.push([0.0, 0.0, -1.0]);
    colours.push([1.0; 4]);
    uvs.push([0.5; 2]);
    for ring in 1..=10 {
        let radius = ring as f32 / 10.0;
        let fade = (1.0 - radius).clamp(0.0, 1.0).powi(2);
        for sector in 0..8 {
            let angle = sector as f32 * (std::f32::consts::TAU / 8.0);
            let (sine, cosine) = angle.sin_cos();
            let x = radius * cosine;
            let y = radius * sine;
            positions.push([x, y, 0.0]);
            normals.push([0.0, 0.0, -1.0]);
            colours.push([quantized_vertex_fade(fade); 4]);
            uvs.push([0.5 + 0.5 * x, 0.5 + 0.5 * y]);
        }
    }
    let mut indices = Vec::with_capacity(456);
    for sector in 0..8 {
        indices.extend_from_slice(&[0, 1 + sector, 1 + (sector + 1) % 8]);
    }
    for ring in 0..9 {
        for sector in 0..7 {
            let first = 1 + ring * 8 + sector;
            let next = 1 + ring * 8 + (sector + 1) % 8;
            indices.extend_from_slice(&[first, next, first + 8, first + 8, next, next + 8]);
        }
        let first = 1 + ring * 8;
        indices.extend_from_slice(&[
            first,
            first + 7,
            first + 8,
            first + 8,
            first + 7,
            first + 15,
        ]);
    }
    water_ray_mesh(positions, normals, colours, uvs, indices)
}

fn quantized_vertex_fade(fade: f32) -> f32 {
    ((f64::from(fade) * 255.0 + 0.5) as u8) as f32 / 255.0
}

fn water_ray_mesh(
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colours: Vec<[f32; 4]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
) -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colours)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}
