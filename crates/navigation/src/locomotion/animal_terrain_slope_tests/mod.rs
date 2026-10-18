use bevy::prelude::*;
use openzt2_game_data::terrain::TerrainGrid;
use openzt2_game_data::terrain::TerrainPresentation;
use openzt2_game_data::terrain::TerrainSample;
use openzt2_game_data::terrain::TerrainSlopeTile;
use openzt2_game_data::terrain::TerrainUnits;
use openzt2_game_data::terrain::TerrainWaterDepth;

use crate::locomotion::pathfinding::plan_route;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::RouteScratch;
use openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph;

fn terrain_with_ridge(rows: std::ops::RangeInclusive<usize>) -> TerrainGrid {
    let mut samples = vec![
        TerrainSample {
            height_relative_to_base_metres: 0.0,
            biome_index: 0,
            has_ground_cover: false,
            water_depth: TerrainWaterDepth::Dry,
            water_surface_flag: false,
            excludes_elevated_path: false,
            support_floor_metres: None,
            height_cap_metres: None,
            height_is_linked: false,
            has_child_height_link: false,
        };
        25
    ];
    for row in rows {
        samples[row * 5 + 2].height_relative_to_base_metres = 2.0;
    }
    TerrainGrid {
        width: 5,
        height: 5,
        sector_columns: 1,
        sector_rows: 1,
        units: TerrainUnits {
            cell_size_metres: 1.0,
            base_height_metres: 0.0,
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
        samples,
        water_regions: Vec::new(),
        waterfall: None,
        slope_tiles: vec![TerrainSlopeTile {
            uses_alternate_diagonal: false,
        }],
    }
}

fn route_across_terrain(
    graph: &TerrainDerivedNavigationGraph,
    maximum_slope: Option<f32>,
    destination: Vec3,
) -> Result<Route, NavigationFailure> {
    let capacity = graph.navigation_node_count() + 1;
    let mut route = Route::with_capacity(capacity);
    let mut scratch = RouteScratch::default();
    scratch.ensure_capacity(graph.navigation_node_count(), capacity);
    plan_route(
        graph,
        |_, _| false,
        |_, _| false,
        &NavAgent {
            radius_m: 0.25,
            max_speed_mps: 1.0,
            acceleration_mps2: 1.0,
            capabilities: NavFlags::ANIMAL,
        },
        LocomotionMode::Ground,
        maximum_slope,
        Vec3::new(0.5, 0.0, -1.5),
        destination,
        &mut route,
        &mut scratch,
    )?;
    Ok(route)
}

#[test]
fn equal_radius_animals_choose_different_routes_around_a_ridge() {
    let graph =
        TerrainDerivedNavigationGraph::build_from_authored_terrain_grid(&terrain_with_ridge(1..=2))
            .unwrap();
    let destination = Vec3::new(3.5, 0.0, -1.5);
    let cautious = route_across_terrain(&graph, Some(0.3), destination).unwrap();
    let climber = route_across_terrain(&graph, Some(1.5), destination).unwrap();
    assert!(cautious.points.iter().any(|point| point.z < -3.0));
    assert!(climber.points.iter().all(|point| point.z == -1.5));
    assert_eq!(cautious.points.last(), Some(&destination));
}

#[test]
fn slope_barrier_blocks_routes_until_terrain_geometry_is_refreshed() {
    let mut graph =
        TerrainDerivedNavigationGraph::build_from_authored_terrain_grid(&terrain_with_ridge(0..=4))
            .unwrap();
    let destination = Vec3::new(3.5, 0.0, -1.5);
    assert!(matches!(
        route_across_terrain(&graph, Some(0.3), destination),
        Err(NavigationFailure::NoRoute)
    ));
    // Flatten a complete corridor across the ridge through the same graph
    // mutation used by terrain-edit refresh, without changing agent clearance.
    for x in 1..=2 {
        graph
            .update_navigation_node_from_changed_terrain_cell(UVec2::new(x, 1), 0, false, 1.0)
            .unwrap();
    }
    let route = route_across_terrain(&graph, Some(0.3), destination).unwrap();
    assert!(route.points.iter().all(|point| point.z == -1.5));
}

#[test]
fn steep_destination_cannot_snap_to_a_nearby_flat_cell() {
    let graph =
        TerrainDerivedNavigationGraph::build_from_authored_terrain_grid(&terrain_with_ridge(0..=4))
            .unwrap();
    let destination = Vec3::new(1.1, 0.0, -1.5);
    assert!(matches!(
        route_across_terrain(&graph, Some(0.3), destination),
        Err(NavigationFailure::OutsideWorld)
    ));
    assert!(route_across_terrain(&graph, None, destination).is_ok());
}

#[test]
fn graph_slope_respects_authored_diagonals_and_reflected_source_rows() {
    let mut terrain = terrain_with_ridge(0..=0);
    terrain.samples[2].height_relative_to_base_metres = 0.0;
    terrain.samples[6].height_relative_to_base_metres = 2.0;
    for (alternate, expected_normal_y) in [(false, 1.0 / 5.0_f32.sqrt()), (true, 1.0 / 3.0)] {
        terrain.slope_tiles[0].uses_alternate_diagonal = alternate;
        let graph =
            TerrainDerivedNavigationGraph::build_from_authored_terrain_grid(&terrain).unwrap();
        let node = graph
            .calculate_navigation_node_for_terrain_cell([0, 0])
            .unwrap();
        let normal_y = graph
            .find_navigation_node(node)
            .unwrap()
            .minimum_triangle_normal_y;
        assert!((normal_y - expected_normal_y).abs() < 1.0e-6);
    }
}
