use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use openzt2_game_data::terrain::TerrainGrid;
use openzt2_game_data::terrain::TerrainPresentation;
use openzt2_game_data::terrain::TerrainSample;
use openzt2_game_data::terrain::TerrainSlopeTile;
use openzt2_game_data::terrain::TerrainUnits;
use openzt2_game_data::terrain::TerrainWaterDepth;
use openzt2_game_data::terrain::TerrainWaterRegion;
use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;

use openzt2_zoo_state::terrain::dry_surface_eligibility::TerrainDrySurfaceEligibility;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::terrain::terrain_test_asset::terrain_test_asset;
use openzt2_game_assets::terrain::terrain_test_asset::terrain_test_water_mut;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;

fn saved_water_asset() -> TerrainAsset {
    let grid = TerrainGrid {
        width: 17,
        height: 17,
        sector_columns: 1,
        sector_rows: 1,
        units: TerrainUnits {
            cell_size_metres: 1.0,
            base_height_metres: 0.0,
            maximum_height_metres: f32::MAX,
            minimum_water_height_metres: None,
        },
        presentation: TerrainPresentation {
            water_distance_fade: Default::default(),
            water_editing: Default::default(),
            detail_texture: String::new(),
            detail_texture_repetition: 1.0,
            cliff_rise_per_metre: [0.0; 2],
            surface_resolution: 1,
            surface_margin_cells: 0,
            full_alpha_composition: false,
        },
        biomes: Vec::new(),
        samples: vec![
            TerrainSample {
                height_relative_to_base_metres: 0.0,
                biome_index: 0,
                has_ground_cover: false,
                water_depth: TerrainWaterDepth::Deep,
                water_surface_flag: true,
                excludes_elevated_path: false,
                support_floor_metres: None,
                height_cap_metres: None,
                height_is_linked: false,
                has_child_height_link: false,
            };
            17 * 17
        ],
        water_regions: vec![TerrainWaterRegion {
            height_relative_to_base_metres: 1.0,
            water_style_index: 0,
            row_major_cell_indices: (0..17 * 17).collect(),
            depth_ratio: 1.0,
            uses_tank_minimum_depth: false,
        }],
        waterfall: None,
        slope_tiles: vec![
            TerrainSlopeTile {
                uses_alternate_diagonal: false
            };
            16
        ],
    };
    terrain_test_asset(grid)
}

fn edited_water_world() -> (World, Entity, Handle<TerrainAsset>) {
    let mut world = World::new();
    let mut assets = Assets::default();
    let handle = assets.add(saved_water_asset());
    world.insert_resource(assets);
    let document = WorldDefinitionDocument::default();
    let mut definition_assets = Assets::default();
    let definition_handle =
        definition_assets.add(WorldDefinitionAsset::from_test_document(document.clone()));
    world.insert_resource(definition_assets);
    let mut definitions = WorldDefinitions::default();
    definitions.index_test_document(definition_handle, &document);
    world.insert_resource(definitions);
    let chunk = world
        .spawn((
            TerrainChunk {
                source_coord: IVec2::ZERO,
                coord: IVec2::ZERO,
                asset: handle.clone(),
                origin: Vec2::ZERO,
                spacing_m: 1.0,
                side: 17,
            },
            EditedTerrainSamples {
                heights_cm: vec![0; 17 * 17].into_boxed_slice(),
                blend_weights: vec![0; 17 * 17 * 16].into_boxed_slice(),
                ground_cover: vec![0; 17 * 17].into_boxed_slice(),
                water_styles: vec![0; 17 * 17].into_boxed_slice(),
                water_biome_channels: vec![0; 17 * 17].into_boxed_slice(),
                water_cm: vec![100; 17 * 17].into_boxed_slice(),
                water_affiliations: vec![None; 17 * 17].into_boxed_slice(),
            },
        ))
        .id();
    world.insert_resource(TerrainIndex {
        chunks: [(IVec2::ZERO, chunk)].into(),
        origin: Vec2::ZERO,
        chunk_span_m: 16.0,
    });
    (world, chunk, handle)
}

fn on_land(world: &mut World, point: Vec2) -> Option<bool> {
    SystemState::<TerrainDrySurfaceEligibility>::new(world)
        .get(world)
        .unwrap()
        .on_land(point)
}

#[test]
fn live_water_and_ground_edits_supersede_saved_region_membership() {
    let (mut world, chunk, _) = edited_water_world();
    let point = Vec2::splat(6.25);
    // Removed water is dry even though the saved region and wet flags remain.
    assert_eq!(on_land(&mut world, point), Some(true));
    world
        .get_mut::<EditedTerrainSamples>(chunk)
        .unwrap()
        .water_styles[6 * 17 + 6] = 1;
    assert_eq!(on_land(&mut world, point), Some(false));
    assert_eq!(on_land(&mut world, Vec2::splat(7.25)), Some(true));
    world
        .get_mut::<EditedTerrainSamples>(chunk)
        .unwrap()
        .heights_cm
        .fill(200);
    assert_eq!(on_land(&mut world, point), Some(true));
    world
        .get_mut::<EditedTerrainSamples>(chunk)
        .unwrap()
        .heights_cm
        .fill(0);
    assert_eq!(on_land(&mut world, point), Some(false));
    world
        .get_mut::<EditedTerrainSamples>(chunk)
        .unwrap()
        .water_styles
        .fill(0);
    assert_eq!(on_land(&mut world, point), Some(true));
}

#[test]
fn water_painted_on_originally_dry_ground_changes_eligibility() {
    let (mut world, chunk, handle) = edited_water_world();
    {
        let mut assets = world.resource_mut::<Assets<TerrainAsset>>();
        let mut asset = assets.get_mut(&handle).unwrap();
        let (grid, membership) = terrain_test_water_mut(&mut asset);
        grid.water_regions.clear();
        *membership = Box::default();
        for sample in &mut grid.samples {
            sample.water_depth = TerrainWaterDepth::Dry;
            sample.water_surface_flag = false;
        }
    }
    let point = Vec2::splat(6.25);
    assert_eq!(on_land(&mut world, point), Some(true));
    world
        .get_mut::<EditedTerrainSamples>(chunk)
        .unwrap()
        .water_styles
        .fill(1);
    assert_eq!(on_land(&mut world, point), Some(false));
    world
        .get_mut::<EditedTerrainSamples>(chunk)
        .unwrap()
        .water_styles
        .fill(0);
    assert_eq!(on_land(&mut world, point), Some(true));
    world.entity_mut(chunk).remove::<EditedTerrainSamples>();
    assert_eq!(on_land(&mut world, point), Some(true));
}

#[test]
fn saved_water_surface_stays_at_region_level_over_a_sloped_bed() {
    let mut asset = saved_water_asset();
    let (grid, _) = terrain_test_water_mut(&mut asset);
    for (index, sample) in grid.samples.iter_mut().enumerate() {
        sample.height_relative_to_base_metres = (index % 17) as f32 * 0.1;
    }
    let chunk = TerrainChunk {
        source_coord: IVec2::ZERO,
        coord: IVec2::ZERO,
        asset: Handle::default(),
        origin: Vec2::ZERO,
        spacing_m: 1.0,
        side: 17,
    };
    for x in [0.0, 4.0, 9.5, 15.0] {
        let point = openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain(
            &chunk,
            &asset,
            None,
            Vec2::new(x, 8.0),
        )
        .unwrap();
        assert_eq!(point.water_height_m, Some(1.0));
    }
}
