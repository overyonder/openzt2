use openzt2_game_data::terrain::TerrainGrid;

use crate::plugins::terrain::terrain_navigation_cell_queries::average_terrain_corner_samples_into_navigation_cell_height_and_water;

use super::{
    locomotion_types::NavFlags,
    terrain_derived_navigation_graph_types::{
        TerrainDerivedNavigationGraph, TerrainDerivedNavigationGraphNode,
    },
};

impl TerrainDerivedNavigationGraph {
    pub(super) fn build_from_authored_terrain_grid(
        authored_terrain_grid: &TerrainGrid,
    ) -> Option<Self> {
        let terrain_sample_column_count = usize::try_from(authored_terrain_grid.width).ok()?;
        let terrain_sample_row_count = usize::try_from(authored_terrain_grid.height).ok()?;
        let terrain_tile_column_count =
            usize::try_from(authored_terrain_grid.sector_columns).ok()?;
        let terrain_tile_row_count = usize::try_from(authored_terrain_grid.sector_rows).ok()?;
        let terrain_cell_column_count = terrain_sample_column_count.checked_sub(1)?;
        let terrain_cell_row_count = terrain_sample_row_count.checked_sub(1)?;
        if terrain_tile_column_count == 0
            || terrain_tile_row_count == 0
            || terrain_cell_column_count % terrain_tile_column_count != 0
            || terrain_cell_row_count % terrain_tile_row_count != 0
            || authored_terrain_grid.samples.len()
                != terrain_sample_column_count.checked_mul(terrain_sample_row_count)?
        {
            return None;
        }
        let terrain_cell_columns_per_tile = terrain_cell_column_count / terrain_tile_column_count;
        let terrain_cell_rows_per_tile = terrain_cell_row_count / terrain_tile_row_count;
        if terrain_cell_columns_per_tile == 0
            || terrain_cell_columns_per_tile != terrain_cell_rows_per_tile
        {
            return None;
        }
        let terrain_cells_per_tile = u16::try_from(terrain_cell_columns_per_tile).ok()?;
        let terrain_cell_size_centimetres = authored_terrain_grid.units.cell_size_metres * 100.0;
        if !terrain_cell_size_centimetres.is_finite()
            || !(1.0..=f32::from(u16::MAX)).contains(&terrain_cell_size_centimetres.round())
            || !authored_terrain_grid.units.base_height_metres.is_finite()
        {
            return None;
        }
        // The preceding finite range check makes this float-to-integer
        // conversion exact for every accepted value.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let terrain_cell_size_centimetres = terrain_cell_size_centimetres.round() as u16;
        let terrain_tile_size_centimetres = u32::from(terrain_cell_size_centimetres)
            .checked_mul(u32::from(terrain_cells_per_tile))?;
        let terrain_tile_grid_dimensions = [
            authored_terrain_grid.sector_columns,
            authored_terrain_grid.sector_rows,
        ];
        let navigation_node_count =
            terrain_cell_column_count.checked_mul(terrain_cell_row_count)?;
        u32::try_from(navigation_node_count).ok()?;
        let mut navigation_nodes = Vec::with_capacity(navigation_node_count);

        for tile_z in 0..terrain_tile_grid_dimensions[1] {
            for tile_x in 0..terrain_tile_grid_dimensions[0] {
                for local_x in 0..u32::from(terrain_cells_per_tile) {
                    for local_z in 0..u32::from(terrain_cells_per_tile) {
                        let x = tile_x * u32::from(terrain_cells_per_tile) + local_x;
                        let z = tile_z * u32::from(terrain_cells_per_tile) + local_z;
                        let indexes = [
                            z as usize * terrain_sample_column_count + x as usize,
                            z as usize * terrain_sample_column_count + x as usize + 1,
                            (z as usize + 1) * terrain_sample_column_count + x as usize,
                            (z as usize + 1) * terrain_sample_column_count + x as usize + 1,
                        ];
                        let mut samples = [(0_i64, false, 0_u16); 4];
                        for (sample, index) in samples.iter_mut().zip(indexes) {
                            let source = authored_terrain_grid.samples.get(index)?;
                            let height = (source.height_relative_to_base_metres
                                + authored_terrain_grid.units.base_height_metres)
                                * 100.0;
                            if !height.is_finite() {
                                return None;
                            }
                            // Preserve the source-lowering f32 rounding
                            // before the checked four-corner reduction.
                            #[allow(clippy::cast_possible_truncation)]
                            let height = height.round() as i64;
                            let water = source.water_depth
                                != openzt2_game_data::terrain::TerrainWaterDepth::Dry;
                            let water_depth_centimetres = water
                                .then(|| {
                                    authored_terrain_grid
                                        .biomes
                                        .get(usize::from(source.biome_index))
                                })
                                .flatten()
                                .and_then(|biome| match source.water_depth {
                                    openzt2_game_data::terrain::TerrainWaterDepth::Dry => None,
                                    openzt2_game_data::terrain::TerrainWaterDepth::Shallow => {
                                        biome.shallow_water_depth_mm
                                    }
                                    openzt2_game_data::terrain::TerrainWaterDepth::Deep => {
                                        biome.deep_water_depth_mm.or(biome.shallow_water_depth_mm)
                                    }
                                })
                                .and_then(|millimetres| u16::try_from(millimetres.max(0) / 10).ok())
                                .unwrap_or(0);
                            *sample = (height, water, water_depth_centimetres);
                        }
                        let (height_centimetres, water) =
                            average_terrain_corner_samples_into_navigation_cell_height_and_water(
                                samples.map(|(height, water, _)| (height, water)),
                            )?;
                        let water_depth_centimetres = water
                            .then(|| {
                                samples
                                    .iter()
                                    .map(|(_, _, depth)| *depth)
                                    .min()
                                    .unwrap_or(0)
                            })
                            .unwrap_or(0);
                        let mut eligibility_flags = NavFlags(NavFlags::ELIGIBILITY_BITS);
                        if water {
                            eligibility_flags.0 |= NavFlags::WATER.0;
                        }
                        navigation_nodes.push(TerrainDerivedNavigationGraphNode {
                            height_centimetres,
                            clearance_centimetres: terrain_cell_size_centimetres,
                            water_depth_centimetres,
                            eligibility_flags,
                        });
                    }
                }
            }
        }
        (navigation_nodes.len() == navigation_node_count).then_some(Self {
            terrain_cell_size_centimetres,
            terrain_cells_per_tile,
            terrain_tile_size_centimetres,
            terrain_tile_grid_dimensions,
            navigation_nodes: navigation_nodes.into_boxed_slice(),
        })
    }
}
