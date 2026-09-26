use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::habitat_terrain_summary_calculation::{
    calculate_habitat_terrain_summary_from_topology_cells, HabitatTopologyCellTerrainFact,
};

#[test]
fn topology_cell_terrain_facts_produce_exact_land_water_and_sorted_biome_areas() {
    let topology_cells = [IVec2::new(0, 0), IVec2::new(0, 1), IVec2::new(1, 0)];
    let forest_biome_identifier = AssetId([1; 16]);
    let desert_biome_identifier = AssetId([2; 16]);
    let habitat_summary = calculate_habitat_terrain_summary_from_topology_cells(
        &topology_cells,
        4.0,
        false,
        &mut |topology_cell| {
            Some(HabitatTopologyCellTerrainFact {
                water: topology_cell == IVec2::new(0, 1),
                biome: Some(if topology_cell.x == 0 {
                    forest_biome_identifier
                } else {
                    desert_biome_identifier
                }),
            })
        },
    );
    assert_eq!(habitat_summary.land_area_square_metres, 8.0);
    assert_eq!(habitat_summary.water_area_square_metres, 4.0);
    assert_eq!(
        habitat_summary.biome_areas_square_metres.as_ref(),
        [
            (forest_biome_identifier, 8.0),
            (desert_biome_identifier, 4.0),
        ],
    );
}
