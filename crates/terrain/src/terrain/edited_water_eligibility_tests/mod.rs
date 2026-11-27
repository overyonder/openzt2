use bevy::ecs::system::SystemId;
use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use openzt2_game_data::terrain::TerrainBiome;
use openzt2_game_data::terrain::TerrainGrid;
use openzt2_game_data::terrain::TerrainPresentation;
use openzt2_game_data::terrain::TerrainSample;
use openzt2_game_data::terrain::TerrainSlopeTile;
use openzt2_game_data::terrain::TerrainUnits;
use openzt2_game_data::terrain::TerrainWaterDepth;
use openzt2_game_data::terrain::TerrainWaterRegion;
use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

use openzt2_zoo_state::terrain::dry_surface_eligibility::TerrainDrySurfaceEligibility;
use openzt2_zoo_state::terrain::terrain_edit_data_types::TerrainWaterAffiliation;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterRegionEdit, TerrainWaterRegionSource,
};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
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
        biomes: vec![TerrainBiome {
            name: String::new(),
            ground_texture: String::new(),
            cover_texture: String::new(),
            mix_mask: String::new(),
            cliff_textures: Default::default(),
            ground_brushes: Default::default(),
            cover_brushes: Default::default(),
            ground_no_blend_brushes: Default::default(),
            cover_no_blend_brushes: Default::default(),
            draw_order: 0,
            shallow_water_depth_mm: None,
            deep_water_depth_mm: Some(1000),
            water_shore_offset_mm: Some(0),
            water_presentation: None,
        }],
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

/// One 17-sample chunk at source coordinate (0, 0). Source rows run along
/// negative world z, so the chunk spans world z from -16 to 0.
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
    let root = world
        .spawn((
            WorldRoot {
                scenario: default(),
            },
            TopologyGrid::default(),
        ))
        .id();
    let projection = world.register_system(
        crate::terrain::water_regions::authored_membership::project_terrain_water_membership,
    );
    world.insert_resource(MembershipProjection(projection));
    world.init_resource::<Messages<TerrainChanged>>();
    world.init_resource::<Messages<
        openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterCellsChanged,
    >>();
    world.init_resource::<Messages<AssetEvent<TerrainAsset>>>();
    world.init_resource::<Messages<crate::terrain::water_regions::authored_membership::WaterRegionFactsDiscarded>>();
    world.add_observer(
        crate::terrain::water_regions::authored_membership::invalidate_discarded_water_region_facts,
    );
    world.init_resource::<
        Messages<crate::terrain::water_regions::authored_membership::TerrainWaterMembershipChanged>,
    >();
    let chunk = world
        .spawn((
            WorldMember { root },
            TerrainChunk {
                source_coord: IVec2::ZERO,
                coord: IVec2::NEG_Y,
                asset: handle.clone(),
                origin: Vec2::new(0.0, -16.0),
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
    world.entity_mut(root).insert(TerrainIndex {
        chunks: [(IVec2::NEG_Y, chunk)].into(),
        origin: Vec2::ZERO,
        chunk_span_m: 16.0,
    });
    (world, chunk, handle)
}

#[derive(Resource)]
struct MembershipProjection(SystemId);

/// Water membership is projected from the edited samples before eligibility
/// reads it, by one system that sees each edit as a change since its last run.
fn on_land(world: &mut World, point: Vec2) -> Option<bool> {
    let projection = world.resource::<MembershipProjection>().0;
    world.run_system(projection).unwrap();
    let root = world
        .query_filtered::<Entity, With<WorldRoot>>()
        .single(world)
        .unwrap();
    SystemState::<TerrainDrySurfaceEligibility>::new(world)
        .get(world)
        .unwrap()
        .on_land(root, point)
}

fn set_affiliations(
    world: &mut World,
    chunk: Entity,
    affiliation: Option<TerrainWaterAffiliation>,
) {
    world
        .get_mut::<EditedTerrainSamples>(chunk)
        .unwrap()
        .water_affiliations
        .fill(affiliation);
    send_terrain_change(world, chunk, false);
}

fn send_terrain_change(world: &mut World, chunk: Entity, ground_height_changed: bool) {
    let max = UVec2::splat(u32::from(world.get::<TerrainChunk>(chunk).unwrap().side) - 1);
    world.write_message(TerrainChanged {
        transaction: chunk,
        chunk,
        min: UVec2::ZERO,
        max,
        ground_height_changed,
    });
}

#[test]
fn live_water_and_ground_edits_supersede_saved_region_membership() {
    let (mut world, chunk, handle) = edited_water_world();
    world.spawn(TerrainWaterRegionSource::new(handle, 0));
    let point = Vec2::new(6.25, -6.25);
    // Removed water is dry even though the saved region and wet flags remain.
    assert_eq!(on_land(&mut world, point), Some(true));
    set_affiliations(
        &mut world,
        chunk,
        Some(TerrainWaterAffiliation::SourceRow(0)),
    );
    assert_eq!(on_land(&mut world, point), Some(false));
    world
        .get_mut::<EditedTerrainSamples>(chunk)
        .unwrap()
        .heights_cm
        .fill(200);
    send_terrain_change(&mut world, chunk, true);
    assert_eq!(on_land(&mut world, point), Some(true));
    world
        .get_mut::<EditedTerrainSamples>(chunk)
        .unwrap()
        .heights_cm
        .fill(0);
    send_terrain_change(&mut world, chunk, true);
    assert_eq!(on_land(&mut world, point), Some(false));
    set_affiliations(&mut world, chunk, None);
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
    let painted = world
        .spawn(TerrainWaterRegionEdit {
            terrain: handle,
            biome_index: 0,
            surface_height_metres: 1.0,
            depth_ratio: 1.0,
            uses_tank_minimum_depth: false,
        })
        .id();
    let point = Vec2::new(6.25, -6.25);
    assert_eq!(on_land(&mut world, point), Some(true));
    set_affiliations(
        &mut world,
        chunk,
        Some(TerrainWaterAffiliation::Edited(painted)),
    );
    assert_eq!(on_land(&mut world, point), Some(false));
    set_affiliations(&mut world, chunk, None);
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

#[test]
fn scene_caustic_target_follows_water_height_and_reparented_bounds() {
    use super::terrain_water_caustics::scene_projection::project_scene_caustic_targets;
    use openzt2_game_data::scene_prefab::PrefabBoundingSphere;
    use openzt2_world_state::world_spawn::prefab_presentation_types::{
        PrefabCausticPolicy, PrefabLocalRenderBound,
    };
    use openzt2_zoo_state::terrain::terrain_water_caustic_types::TerrainWaterCausticSceneTarget;
    use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
        TerrainWaterMembershipIndex, TerrainWaterRegionEdit,
    };

    let mut app = App::new();
    let mut assets = Assets::default();
    let terrain = assets.add(saved_water_asset());
    app.insert_resource(assets)
        .add_message::<AssetEvent<TerrainAsset>>()
        .add_message::<crate::terrain::water_regions::authored_membership::TerrainWaterMembershipChanged>()
        .add_systems(Update, project_scene_caustic_targets);
    let owner = app
        .world_mut()
        .spawn(TerrainWaterRegionEdit {
            terrain: terrain.clone(),
            biome_index: 0,
            surface_height_metres: 0.0,
            depth_ratio: 1.0,
            uses_tank_minimum_depth: false,
        })
        .id();
    let index = TerrainWaterMembershipIndex::new(
        terrain.id(),
        17,
        17,
        vec![Some(owner); 17 * 17].into_boxed_slice(),
        vec![[Some(owner); 4]; 16].into_boxed_slice(),
        true,
    )
    .unwrap();
    app.world_mut().spawn(index);
    let mut spawn_model = || {
        app.world_mut()
            .spawn((
                PrefabCausticPolicy { receives: true },
                Transform::from_xyz(2.0, 4.0, -2.0),
            ))
            .id()
    };
    let first = spawn_model();
    let second = spawn_model();
    let geometry = app
        .world_mut()
        .spawn((
            PrefabLocalRenderBound(PrefabBoundingSphere {
                center: [0.0; 3],
                radius: 1.0,
            }),
            Transform::IDENTITY,
            ChildOf(first),
        ))
        .id();
    let target = |app: &App, root| {
        app.world()
            .get::<TerrainWaterCausticSceneTarget>(root)
            .map(|t| t.water_owner)
    };
    app.update();
    assert_eq!(target(&app, first), None);
    app.world_mut()
        .get_mut::<TerrainWaterRegionEdit>(owner)
        .unwrap()
        .surface_height_metres = 3.5;
    app.update();
    assert_eq!(target(&app, first), Some(owner));
    app.world_mut().entity_mut(geometry).insert(ChildOf(second));
    app.update();
    assert_eq!(target(&app, first), None);
    assert_eq!(target(&app, second), Some(owner));
    app.world_mut()
        .entity_mut(second)
        .remove::<PrefabCausticPolicy>();
    app.update();
    assert_eq!(target(&app, second), None);
}
