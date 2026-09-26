use crate::assets::model_source::conversion_error::ConversionError;
use crate::assets::model_source::gltf_binary_buffer::GltfBinaryBuffer;
// Lowering of parsed Blue Fang terrain into the grid and GLB mesh.

use std::collections::BTreeMap;

use gltf::json::{
    accessor::Type,
    buffer::Target,
    mesh::{Mesh, Mode, Primitive, Semantic},
    scene::{Node, Scene},
    validation::Checked,
    Root,
};
use openzt2_game_data::terrain::{
    TerrainBiome, TerrainGrid, TerrainPresentation, TerrainSample, TerrainUnits, TerrainWaterDepth,
    TerrainWaterfall,
};

use super::{
    blue_fang_terrain_source_parsing::blue_fang_terrain_cell_size_metres,
    blue_fang_terrain_source_types::{ParsedBlueFangTerrain, ParsedBlueFangTerrainSourceData},
    terrain_chunk_mesh_geometry::{terrain_chunk_triangle_indices, terrain_height_field_normals},
};

pub(in crate::assets) struct LoweredBlueFangTerrain {
    pub(in crate::assets) grid: TerrainGrid,
    pub(in crate::assets) glb: Vec<u8>,
}

/// Lowers one original terrain grid into an ordinary indexed GLB mesh.
/// `cell_size_metres` and `base_height_metres` are the authored BFTerrain
/// values resolved from the map's XML documents.
pub(in crate::assets) fn lower_blue_fang_terrain_to_domain_grid_and_glb(
    terrain: ParsedBlueFangTerrain,
    base_height_metres: f32,
    presentation: TerrainPresentation,
    biomes: Vec<TerrainBiome>,
    waterfall: Option<TerrainWaterfall>,
) -> anyhow::Result<LoweredBlueFangTerrain> {
    let cell_size_metres = blue_fang_terrain_cell_size_metres(terrain.0.header)?;
    lower_blue_fang_terrain_with_validated_units(
        terrain.0,
        cell_size_metres,
        base_height_metres,
        presentation,
        biomes,
        waterfall,
    )
    .map_err(Into::into)
}

fn lower_blue_fang_terrain_with_validated_units(
    terrain: ParsedBlueFangTerrainSourceData,
    cell_size_metres: f32,
    base_height_metres: f32,
    presentation: TerrainPresentation,
    biomes: Vec<TerrainBiome>,
    waterfall: Option<TerrainWaterfall>,
) -> Result<LoweredBlueFangTerrain, ConversionError> {
    if !cell_size_metres.is_finite() || cell_size_metres <= 0.0 || !base_height_metres.is_finite() {
        return Err(ConversionError::InvalidValue(
            "invalid terrain unit contract",
        ));
    }
    if terrain.biome_names.len() != biomes.len()
        || terrain
            .biome_names
            .iter()
            .zip(&biomes)
            .any(|(source, biome)| !source.eq_ignore_ascii_case(&biome.name))
    {
        return Err(ConversionError::InvalidValue(
            "terrain biome bindings do not match the DAT token table",
        ));
    }
    if biomes.len() > 16
        || terrain.samples.iter().any(|sample| {
            sample.biome as usize >= biomes.len()
                || sample.ground_cover > 1
                || sample.water_type > 2
        })
    {
        return Err(ConversionError::InvalidValue(
            "terrain sample references an invalid biome, cover, or water type",
        ));
    }
    let width = usize::try_from(terrain.header.width)
        .map_err(|_| ConversionError::InvalidSource("terrain width exceeds usize"))?;
    let height = usize::try_from(terrain.header.height)
        .map_err(|_| ConversionError::InvalidSource("terrain height exceeds usize"))?;
    if terrain.header.sector_columns == 0
        || terrain.header.sector_rows == 0
        || (width - 1) % terrain.header.sector_columns as usize != 0
        || (height - 1) % terrain.header.sector_rows as usize != 0
    {
        return Err(ConversionError::InvalidSource(
            "terrain grid does not divide into authored sectors",
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
                sample.height + base_height_metres,
                z as f32 * cell_size_metres,
            ]
        })
        .collect::<Vec<_>>();
    let mut gltf_root = Root::default();
    let mut binary_buffer = GltfBinaryBuffer::default();
    let chunk_width = (width - 1) / terrain.header.sector_columns as usize;
    let chunk_height = (height - 1) / terrain.header.sector_rows as usize;
    let mut terrain_chunk_node_indices = Vec::new();
    for chunk_z in 0..terrain.header.sector_rows as usize {
        for chunk_x in 0..terrain.header.sector_columns as usize {
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
                    terrain.samples[source_z * width + source_x].biome
                })
                .collect::<Vec<_>>();
            let cover_values = samples
                .iter()
                .map(|(source_x, source_z, _, _)| {
                    u32::from(terrain.samples[source_z * width + source_x].ground_cover)
                })
                .collect::<Vec<_>>();
            let water_values = samples
                .iter()
                .map(|(source_x, source_z, _, _)| {
                    terrain.samples[source_z * width + source_x].water_type
                })
                .collect::<Vec<_>>();
            let chunk_indices = terrain_chunk_triangle_indices(
                &terrain,
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
    let grid = TerrainGrid {
        width: terrain.header.width,
        height: terrain.header.height,
        sector_columns: terrain.header.sector_columns,
        sector_rows: terrain.header.sector_rows,
        units: TerrainUnits {
            cell_size_metres,
            base_height_metres,
        },
        presentation,
        biomes,
        samples: terrain
            .samples
            .into_iter()
            .map(|sample| {
                let water_depth = match sample.water_type {
                    0 => TerrainWaterDepth::Dry,
                    1 => TerrainWaterDepth::Shallow,
                    2 => TerrainWaterDepth::Deep,
                    _ => {
                        return Err(ConversionError::InvalidValue(
                            "terrain sample has invalid water depth",
                        ));
                    }
                };
                Ok(TerrainSample {
                    height_relative_to_base_metres: sample.height,
                    biome_index: u8::try_from(sample.biome).map_err(|_| {
                        ConversionError::InvalidValue("terrain sample biome index exceeds u8")
                    })?,
                    has_ground_cover: sample.ground_cover != 0,
                    water_depth,
                    water_surface_flag: sample.water_surface_flag,
                    excludes_elevated_path: sample.excludes_elevated_path,
                    support_floor_metres: sample.minimum_height,
                    height_cap_metres: sample.maximum_height,
                    height_is_linked: sample.linked_height_shape.is_some_and(|value| value != 0),
                    has_child_height_link: sample
                        .constraint_flag
                        .is_some_and(|value| value & 1 != 0),
                })
            })
            .collect::<Result<Vec<_>, ConversionError>>()?,
        water_regions: terrain.water_regions,
        waterfall,
        slope_tiles: terrain.slope_tiles,
    };
    Ok(LoweredBlueFangTerrain { grid, glb: mesh })
}
