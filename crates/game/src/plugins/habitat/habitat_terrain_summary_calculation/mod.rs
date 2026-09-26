use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::habitat_types::HabitatSummary;

#[derive(Debug, Clone, Copy)]
pub(super) struct HabitatTopologyCellTerrainFact {
    pub water: bool,
    pub biome: Option<AssetId>,
}

pub(super) fn calculate_habitat_terrain_summary_from_topology_cells(
    topology_cells: &[IVec2],
    topology_cell_area_square_metres: f32,
    boundary_is_breached: bool,
    read_terrain_cell_fact: &mut impl FnMut(IVec2) -> Option<HabitatTopologyCellTerrainFact>,
) -> HabitatSummary {
    let mut land_area_square_metres = 0.0;
    let mut water_area_square_metres = 0.0;
    let mut biome_areas_square_metres: Vec<(AssetId, f32)> = Vec::new();
    for topology_cell in topology_cells.iter().copied() {
        let Some(terrain_cell_fact) = read_terrain_cell_fact(topology_cell) else {
            continue;
        };
        if terrain_cell_fact.water {
            water_area_square_metres += topology_cell_area_square_metres;
        } else {
            land_area_square_metres += topology_cell_area_square_metres;
        }
        if let Some(biome_identifier) = terrain_cell_fact.biome {
            match biome_areas_square_metres
                .binary_search_by_key(&biome_identifier.0, |(identifier, _)| identifier.0)
            {
                Ok(biome_index) => {
                    biome_areas_square_metres[biome_index].1 += topology_cell_area_square_metres;
                }
                Err(insertion_index) => biome_areas_square_metres.insert(
                    insertion_index,
                    (biome_identifier, topology_cell_area_square_metres),
                ),
            }
        }
    }
    HabitatSummary {
        land_area_square_metres,
        water_area_square_metres,
        biome_areas_square_metres: biome_areas_square_metres.into_boxed_slice(),
        boundary_is_breached,
    }
}
