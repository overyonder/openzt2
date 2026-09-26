use openzt2_game_data::world_definitions::object_placement::{
    FootprintCell, FootprintCellFlags, PlaceableDefinition, PlacementConstraints,
};
use openzt2_game_data::AssetId;

fn create_object_placeable_definition_test_fixture(
    constraints: PlacementConstraints,
) -> PlaceableDefinition {
    PlaceableDefinition {
        id: AssetId([7; 16]),
        weight: 1.0,
        footprint: vec![FootprintCell {
            offset: [0, 0],
            flags: FootprintCellFlags::EMPTY,
        }],
        pivot_cm: [0, 0],
        diagonal_footprint: vec![FootprintCell {
            offset: [0, 0],
            flags: FootprintCellFlags::EMPTY,
        }],
        diagonal_pivot_cm: [0, 0],
        rotation_increment_degrees: 90,
        constraints,
        max_slope_permille: 250,
        minimum_headroom_metres: 2.0,
        apply_height_modifier: true,
        price_cents: 1_250,
        unlock: AssetId([9; 16]),
        entrances: Vec::new(),
        moving_footprint: false,
        automatic_footprint: false,
        ground_paths_block_placement: false,
    }
}

mod object_placement_occupancy_index_tests;
mod object_placement_preview_interpolation_tests;

mod object_placement_transform_validation_tests;
