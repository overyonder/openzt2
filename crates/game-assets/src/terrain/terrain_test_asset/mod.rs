use openzt2_game_data::terrain::TerrainGrid;

use super::terrain_asset_types_and_borrowing_queries::TerrainAsset;

pub fn terrain_test_asset(grid: TerrainGrid) -> TerrainAsset {
    TerrainAsset {
        authored_water_region_membership: TerrainAsset::index_authored_water_region_membership(
            &grid,
        )
        .unwrap(),
        canonical_terrain_grid: grid,
        tank_water_materials: Default::default(),
        chunk_meshes: Default::default(),
        terrain_effect_material: Default::default(),
        water_bump_combination_effect_material: Default::default(),
        water_surface_effect_material: Default::default(),
        water_low_detail_materials: Default::default(),
        water_medium_detail_materials: Default::default(),
        waterfall_decal_effect_material: Default::default(),
        texture_images_by_normalized_source_path: Default::default(),
        waterfall_scene_prefab: None,
        waterfall_audio: None,
        shoreline_ripples: Default::default(),
        water_materials_by_normalized_source_path: Default::default(),
        water_particle_effects_by_normalized_system: Default::default(),
        named_water_particle_sources: Default::default(),
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

/// The source-path texture table, for tests that compose terrain surfaces.
pub fn terrain_test_textures_mut(
    asset: &mut TerrainAsset,
) -> &mut bevy::platform::collections::HashMap<String, bevy::prelude::Handle<bevy::prelude::Image>>
{
    &mut asset.texture_images_by_normalized_source_path
}
