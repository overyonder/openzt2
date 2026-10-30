use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::terrain::terrain_chunk_identity_type::TerrainChunkId;
use openzt2_zoo_state::terrain::terrain_chunk_types::{
    EditedTerrainSamples, TerrainChunk, TerrainIndex,
};
use openzt2_zoo_state::terrain::terrain_edit_rectangle_operations::{
    apply_terrain_sample_rectangle, materialize_complete_edited_terrain_sample_overlay,
};
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEdit;
use openzt2_zoo_state::topology::topology_graph_types::{FenceEdge, TopologyGrid, TopologyNode};

use super::aquatic_simulation_types::{Tank, TankGeometry};
use super::tank_floor_terrain_excavation::{
    excavate_changed_tank_floors, inside_walls_beyond, TankFloorExcavation,
};
use super::tank_geometry_edit_tests::install_flat_terrain;

#[test]
fn tank_floor_excavation_keeps_samples_on_and_beside_the_walls() {
    let corners = [
        Vec2::new(0.0, 0.0),
        Vec2::new(4.0, 0.0),
        Vec2::new(4.0, 4.0),
        Vec2::new(0.0, 4.0),
    ];
    let walls = (0..4)
        .map(|side| [corners[side], corners[(side + 1) % 4]])
        .collect::<Vec<_>>();
    let inside = (0..=8)
        .flat_map(|z| (0..=8).map(move |x| Vec2::new(x as f32 * 0.5, z as f32 * 0.5)))
        .filter(|point| inside_walls_beyond(*point, &walls, 0.5))
        .collect::<Vec<_>>();
    // Samples every 0.5 m with a 0.5 m margin: only 1.0..=3.0 on each axis.
    assert_eq!(inside.len(), 25);
    assert!(inside.contains(&Vec2::new(1.0, 1.0)));
    assert!(!inside.contains(&Vec2::new(0.5, 2.0)));
}

/// Applies the one pending excavation to the terrain, as the terrain domain
/// would, and returns the heights it changed, in centimetres.
fn apply_excavation(app: &mut App, step: &str) -> Vec<i16> {
    let mut excavations = app
        .world_mut()
        .query_filtered::<(Entity, &TerrainEdit), With<TankFloorExcavation>>();
    let (entity, deltas) = excavations
        .iter(app.world())
        .map(|(entity, edit)| (entity, edit.deltas.clone()))
        .next()
        .unwrap_or_else(|| panic!("no excavation after {step}"));
    app.world_mut().despawn(entity);
    let chunk = app.world().resource::<TerrainIndex>().chunks[&IVec2::ZERO];
    let terrain = app.world().get::<TerrainChunk>(chunk).unwrap().clone();
    let mut overlay = app
        .world()
        .get::<EditedTerrainSamples>(chunk)
        .cloned()
        .unwrap_or_else(|| {
            let assets = app.world().resource::<Assets<TerrainAsset>>();
            materialize_complete_edited_terrain_sample_overlay(
                &terrain,
                assets.get(&terrain.asset).unwrap(),
            )
        });
    let mut heights = Vec::new();
    for delta in &deltas {
        apply_terrain_sample_rectangle(
            &mut overlay,
            terrain.side,
            delta.min,
            delta.max,
            &delta.after,
        );
        heights.extend(
            delta
                .before
                .iter()
                .zip(delta.after.iter())
                .filter(|(before, after)| before != after)
                .map(|(_, after)| after.height_cm),
        );
    }
    app.world_mut().entity_mut(chunk).insert(overlay);
    heights
}

#[test]
fn a_tank_floor_is_dug_into_the_terrain_and_filled_back_when_the_tank_goes() {
    let mut app = App::new();
    app.init_resource::<Assets<
        openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    >>()
    .init_resource::<TopologyGrid>()
    .add_systems(Update, excavate_changed_tank_floors);
    install_flat_terrain(&mut app, 7);
    let chunk = app.world().resource::<TerrainIndex>().chunks[&IVec2::ZERO];
    app.world_mut().entity_mut(chunk).insert(TerrainChunkId(0));
    let corners = [
        IVec3::ZERO,
        IVec3::new(5, 0, 0),
        IVec3::new(5, 5, 0),
        IVec3::new(0, 5, 0),
    ]
    .map(|cell| app.world_mut().spawn(TopologyNode { cell }).id());
    let walls = (0..4)
        .map(|index| {
            app.world_mut()
                .spawn(FenceEdge {
                    definition: Default::default(),
                    a: corners[index],
                    b: corners[(index + 1) % 4],
                })
                .id()
        })
        .collect::<Box<[_]>>();
    let tank = app
        .world_mut()
        .spawn((
            Tank {
                definition: Default::default(),
            },
            TankGeometry {
                floor_height: -2.0,
                wall_height: 4.0,
                water_height: 3.0,
                water_height_limit: 1_000_000.0,
                wall_base_height: 0.0,
                area: 25.0,
                volume: 125.0,
            },
            HabitatRegion {
                topology_cells: Box::default(),
                area_square_metres: 25.0,
                boundary_fence_entities: walls,
            },
        ))
        .id();
    app.update();
    // Walls stay inside the one test chunk, whose far edge belongs to the
    // next chunk. Samples 2..=3 lie more than one spacing inside the walls.
    assert_eq!(apply_excavation(&mut app, "digging"), [-200; 4]);
    app.world_mut().entity_mut(tank).remove::<Tank>();
    app.update();
    // The flat test ground lies at zero.
    assert_eq!(apply_excavation(&mut app, "filling"), [0; 4]);
}
