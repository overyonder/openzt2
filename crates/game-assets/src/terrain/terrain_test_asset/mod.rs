use openzt2_game_data::terrain::TerrainGrid;

use super::terrain_asset_types_and_borrowing_queries::TerrainAsset;

pub fn terrain_test_asset(grid: TerrainGrid) -> TerrainAsset {
    TerrainAsset {
        authored_water_region_membership: TerrainAsset::index_authored_water_region_membership(
            &grid,
        )
        .unwrap(),
        canonical_terrain_grid: grid,
        lowered_terrain_model: Default::default(),
        terrain_effect_material: Default::default(),
        water_bump_combination_effect_material: Default::default(),
        water_surface_effect_material: Default::default(),
        waterfall_decal_effect_material: Default::default(),
        texture_images_by_normalized_source_path: Default::default(),
        waterfall_scene_prefab: None,
        waterfall_audio: None,
    }
}

/// The authored grid and its water-region membership, for tests that edit
/// water directly.
pub fn terrain_test_water_mut(asset: &mut TerrainAsset) -> (&mut TerrainGrid, &mut Box<[u32]>) {
    (
        &mut asset.canonical_terrain_grid,
        &mut asset.authored_water_region_membership,
    )
}
