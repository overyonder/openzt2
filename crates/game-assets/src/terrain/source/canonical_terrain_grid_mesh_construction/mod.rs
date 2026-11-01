//! Indexed geometry for a canonical terrain grid.

use super::terrain_chunk_mesh_geometry::{
    terrain_chunk_triangle_indices, terrain_height_field_normals,
};
use crate::model_source::{
    conversion_error::ConversionError, gltf_binary_buffer::GltfBinaryBuffer,
};
use gltf::json::{
    accessor::Type,
    buffer::Target,
    mesh::{Mesh, Mode, Primitive, Semantic},
    scene::{Node, Scene},
    validation::Checked,
    Root,
};
use openzt2_game_data::terrain::{TerrainGrid, TerrainWaterDepth};
use std::collections::BTreeMap;

pub(crate) fn create_canonical_terrain_grid_glb(
    terrain: &TerrainGrid,
) -> Result<Vec<u8>, ConversionError> {
    let width = terrain.width as usize;
    let height = terrain.height as usize;
    let cell_size_metres = terrain.units.cell_size_metres;
    let base_height_metres = terrain.units.base_height_metres;
    let presentation = &terrain.presentation;
    if width < 2
        || height < 2
        || terrain.sector_columns == 0
        || terrain.sector_rows == 0
        || (width - 1) % terrain.sector_columns as usize != 0
        || (height - 1) % terrain.sector_rows as usize != 0
        || (width - 1) % 4 != 0
        || (height - 1) % 4 != 0
        || width.checked_mul(height) != Some(terrain.samples.len())
        || ((width - 1) / 4).checked_mul((height - 1) / 4) != Some(terrain.slope_tiles.len())
        || !cell_size_metres.is_finite()
        || cell_size_metres <= 0.0
        || !base_height_metres.is_finite()
        || terrain.samples.iter().any(|sample| {
            !sample.height_relative_to_base_metres.is_finite()
                || sample.biome_index as usize >= terrain.biomes.len()
        })
    {
        return Err(ConversionError::InvalidValue(
            "invalid terrain geometry grid",
        ));
    }
    let positions = terrain
        .samples
        .iter()
        .enumerate()
        .map(|(index, sample)| {
            let x = index % width;
            let z = index / width;
            [
                x as f32 * cell_size_metres,
                sample.height_relative_to_base_metres + base_height_metres,
                z as f32 * cell_size_metres,
            ]
        })
        .collect::<Vec<_>>();
    let mut gltf_root = Root::default();
    let mut binary_buffer = GltfBinaryBuffer::default();
    let chunk_width = (width - 1) / terrain.sector_columns as usize;
    let chunk_height = (height - 1) / terrain.sector_rows as usize;
    let mut terrain_chunk_node_indices = Vec::new();
    for chunk_z in 0..terrain.sector_rows as usize {
        for chunk_x in 0..terrain.sector_columns as usize {
            let origin_x = chunk_x * chunk_width;
            let origin_z = chunk_z * chunk_height;
            let samples = (0..=chunk_height)
                .flat_map(|local_z| {
                    (0..=chunk_width).map(move |local_x| {
                        (
                            origin_x + local_x,
                            origin_z + chunk_height - local_z,
                            local_x,
                            local_z,
                        )
                    })
                })
                .collect::<Vec<_>>();
            let chunk_position_vectors = samples
                .iter()
                .map(|(source_x, source_z, local_x, local_z)| {
                    let position = positions[source_z * width + source_x];
                    [
                        *local_x as f32 * cell_size_metres,
                        position[1],
                        *local_z as f32 * cell_size_metres,
                    ]
                })
                .collect::<Vec<_>>();
            let chunk_normals = terrain_height_field_normals(
                &chunk_position_vectors,
                chunk_width + 1,
                chunk_height + 1,
                cell_size_metres,
            )
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            let chunk_positions = chunk_position_vectors
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<_>>();
            let margin = f32::from(presentation.surface_margin_cells);
            let composed_width = chunk_width as f32 + margin * 2.0;
            let composed_height = chunk_height as f32 + margin * 2.0;
            let texcoords = samples
                .iter()
                .flat_map(|(_, _, local_x, local_z)| {
                    [
                        *local_x as f32 / composed_width + margin / composed_width,
                        *local_z as f32 / composed_height + margin / composed_height,
                    ]
                })
                .collect::<Vec<_>>();
            let detail_texcoords = samples
                .iter()
                .flat_map(|(_, _, local_x, local_z)| {
                    [
                        (origin_x + *local_x) as f32
                            * cell_size_metres
                            * presentation.detail_texture_repetition,
                        (origin_z + *local_z) as f32
                            * cell_size_metres
                            * presentation.detail_texture_repetition,
                    ]
                })
                .collect::<Vec<_>>();
            let diffuse_colours = vec![1.0_f32; samples.len() * 4];
            let biome_values = samples
                .iter()
                .map(|(source_x, source_z, _, _)| {
                    u32::from(terrain.samples[source_z * width + source_x].biome_index)
                })
                .collect::<Vec<_>>();
            let cover_values = samples
                .iter()
                .map(|(source_x, source_z, _, _)| {
                    u32::from(terrain.samples[source_z * width + source_x].has_ground_cover)
                })
                .collect::<Vec<_>>();
            let water_values = samples
                .iter()
                .map(|(source_x, source_z, _, _)| {
                    match terrain.samples[source_z * width + source_x].water_depth {
                        TerrainWaterDepth::Dry => 0_u32,
                        TerrainWaterDepth::Shallow => 1,
                        TerrainWaterDepth::Deep => 2,
                    }
                })
                .collect::<Vec<_>>();
            let chunk_indices = terrain_chunk_triangle_indices(
                &terrain.slope_tiles,
                width,
                origin_x,
                origin_z,
                chunk_width,
                chunk_height,
            )?;
            let position = binary_buffer.add_f32_accessor(
                &mut gltf_root,
                &chunk_positions,
                Type::Vec3,
                true,
                Some(Target::ArrayBuffer),
            );
            let normal = binary_buffer.add_f32_accessor(
                &mut gltf_root,
                &chunk_normals,
                Type::Vec3,
                false,
                Some(Target::ArrayBuffer),
            );
            let texcoord = binary_buffer.add_f32_accessor(
                &mut gltf_root,
                &texcoords,
                Type::Vec2,
                false,
                Some(Target::ArrayBuffer),
            );
            let detail_texcoord = binary_buffer.add_f32_accessor(
                &mut gltf_root,
                &detail_texcoords,
                Type::Vec2,
                false,
                Some(Target::ArrayBuffer),
            );
            let diffuse_colour = binary_buffer.add_f32_accessor(
                &mut gltf_root,
                &diffuse_colours,
                Type::Vec4,
                false,
                Some(Target::ArrayBuffer),
            );
            let biome = binary_buffer.add_u32_accessor(
                &mut gltf_root,
                &biome_values,
                Type::Scalar,
                Some(Target::ArrayBuffer),
            );
            let cover = binary_buffer.add_u32_accessor(
                &mut gltf_root,
                &cover_values,
                Type::Scalar,
                Some(Target::ArrayBuffer),
            );
            let water = binary_buffer.add_u32_accessor(
                &mut gltf_root,
                &water_values,
                Type::Scalar,
                Some(Target::ArrayBuffer),
            );
            let index = binary_buffer.add_u32_accessor(
                &mut gltf_root,
                &chunk_indices,
                Type::Scalar,
                Some(Target::ElementArrayBuffer),
            );
            let chunk_name = format!("terrain_chunk_{chunk_x}_{chunk_z}");
            let attributes = BTreeMap::from_iter([
                (Checked::Valid(Semantic::Positions), position),
                (Checked::Valid(Semantic::Normals), normal),
                (Checked::Valid(Semantic::TexCoords(0)), texcoord),
                (Checked::Valid(Semantic::TexCoords(1)), detail_texcoord),
                (Checked::Valid(Semantic::Colors(0)), diffuse_colour),
                (
                    Checked::Valid(Semantic::Extras("_BIOME_INDEX".to_owned())),
                    biome,
                ),
                (
                    Checked::Valid(Semantic::Extras("_GROUND_COVER".to_owned())),
                    cover,
                ),
                (
                    Checked::Valid(Semantic::Extras("_WATER_TYPE".to_owned())),
                    water,
                ),
            ]);
            let terrain_chunk_mesh = gltf_root.push(Mesh {
                extensions: None,
                extras: None,
                name: Some(chunk_name.clone()),
                primitives: vec![Primitive {
                    attributes,
                    extensions: None,
                    extras: None,
                    indices: Some(index),
                    material: None,
                    mode: Checked::Valid(Mode::Triangles),
                    targets: None,
                }],
                weights: None,
            });
            terrain_chunk_node_indices.push(gltf_root.push(Node {
                mesh: Some(terrain_chunk_mesh),
                name: Some(chunk_name),
                translation: Some([
                    origin_x as f32 * cell_size_metres,
                    0.0,
                    -((origin_z + chunk_height) as f32 * cell_size_metres),
                ]),
                ..Default::default()
            }));
        }
    }
    let terrain_scene = gltf_root.push(Scene {
        extensions: None,
        extras: None,
        name: None,
        nodes: terrain_chunk_node_indices,
    });
    gltf_root.scene = Some(terrain_scene);
    let mesh = binary_buffer.finish(gltf_root)?;
    Ok(mesh)
}
