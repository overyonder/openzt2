use super::*;
use openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraphNode;

#[test]
fn amphibious_animals_cross_a_water_channel_in_both_directions() {
    let mut nodes = vec![
        TerrainDerivedNavigationGraphNode {
            height_centimetres: 0,
            minimum_triangle_normal_y: 1.0,
            clearance_centimetres: 100,
            water_depth_centimetres: 0,
            eligibility_flags: NavFlags::ANIMAL,
        };
        16
    ];
    // A full column separates the two dry banks: no dry detour exists.
    for node in &mut nodes[4..8] {
        node.eligibility_flags.0 |= NavFlags::WATER.0;
        node.water_depth_centimetres = 200;
    }
    let graph =
        TerrainDerivedNavigationGraph::from_terrain_nodes(100, 4, [1, 1], nodes.into_boxed_slice());
    let mut agent = NavAgent {
        radius_m: 0.25,
        max_speed_mps: 1.0,
        acceleration_mps2: 1.0,
        capabilities: NavFlags::ANIMAL,
    };
    let mut scratch = RouteScratch::default();
    scratch.ensure_capacity(16, 17);
    let mut route = Route::with_capacity(17);
    for amphibious in [false, true] {
        if amphibious {
            agent.capabilities.0 |= NavFlags::AMPHIBIOUS.0;
        }
        for (start, destination, mode) in [
            (
                Vec3::new(0.5, 0.0, -0.5),
                Vec3::new(2.5, 0.0, -0.5),
                LocomotionMode::Ground,
            ),
            (
                Vec3::new(1.5, 2.0, -0.5),
                Vec3::new(0.5, 0.0, -0.5),
                LocomotionMode::Swim,
            ),
        ] {
            route.clear();
            let result = plan_route(
                &graph,
                |_, _| false,
                |_, _| false,
                &agent,
                mode,
                None,
                start,
                destination,
                &mut route,
                &mut scratch,
                crate::locomotion::pathfinding::LONG_PATH_ITERATIONS,
            );
            assert_eq!(
                result.is_ok(),
                amphibious,
                "{amphibious}: {start:?} -> {destination:?}: {result:?}"
            );
        }
    }
}

#[test]
fn ground_endpoint_snapping_rejects_water_but_preserves_dry_approaches() {
    let node = TerrainDerivedNavigationGraphNode {
        height_centimetres: 0,
        minimum_triangle_normal_y: 1.0,
        clearance_centimetres: 100,
        water_depth_centimetres: 0,
        eligibility_flags: NavFlags::default(),
    };
    let mut nodes = vec![node; 16];
    nodes[0].eligibility_flags = NavFlags::STAFF;
    nodes[4].eligibility_flags = NavFlags::WATER;
    let mut graph =
        TerrainDerivedNavigationGraph::from_terrain_nodes(100, 4, [1, 1], nodes.into_boxed_slice());
    let agent = NavAgent {
        radius_m: 0.25,
        max_speed_mps: 1.0,
        acceleration_mps2: 1.0,
        capabilities: NavFlags::STAFF,
    };
    let mut scratch = RouteScratch::default();
    scratch.ensure_capacity(16, 17);
    let mut route = Route::with_capacity(17);
    let mut plan = |graph: &TerrainDerivedNavigationGraph, destination| {
        let result = plan_route(
            graph,
            |_, _| false,
            |_, _| false,
            &agent,
            LocomotionMode::Ground,
            None,
            Vec3::new(0.5, 0.0, -0.5),
            destination,
            &mut route,
            &mut scratch,
            crate::locomotion::pathfinding::LONG_PATH_ITERATIONS,
        );
        if result.is_err() {
            assert!(route.points.is_empty());
        }
        result
    };
    assert_eq!(
        plan(&graph, Vec3::new(1.5, 0.0, -0.5)),
        Err(NavigationFailure::NoRoute)
    );
    // A dry endpoint beyond a water cell is also unsafe after snapping.
    assert_eq!(
        plan(&graph, Vec3::new(2.5, 0.0, -0.5)),
        Err(NavigationFailure::NoRoute)
    );
    assert!(plan(&graph, Vec3::new(0.75, 0.0, -0.5)).is_ok());
    graph.navigation_nodes[4].eligibility_flags = NavFlags::default();
    // Preserve existing dry docking offsets outside the eligible node itself.
    assert!(plan(&graph, Vec3::new(2.5, 0.0, -0.5)).is_ok());
}

#[test]
fn sliced_search_resumes_across_tile_boundaries_and_reuses_dirty_scratch() {
    let node = TerrainDerivedNavigationGraphNode {
        height_centimetres: 0,
        minimum_triangle_normal_y: 1.0,
        clearance_centimetres: 100,
        water_depth_centimetres: 0,
        eligibility_flags: NavFlags::ANIMAL,
    };
    let graph = TerrainDerivedNavigationGraph::from_terrain_nodes(
        100,
        4,
        [2, 2],
        vec![node; 64].into_boxed_slice(),
    );
    let agent = NavAgent {
        radius_m: 0.25,
        max_speed_mps: 1.0,
        acceleration_mps2: 1.0,
        capabilities: NavFlags::ANIMAL,
    };
    let mut scratch = RouteScratch::default();
    scratch.ensure_capacity(64, 65);
    let mut route = Route::with_capacity(65);
    let a = Vec3::new(0.5, 0.0, -0.5);
    let b = Vec3::new(7.5, 0.0, -7.5);
    for (from, to) in [(a, b), (b, a)] {
        route.clear();
        let mut search = RouteSearch::default();
        for slice in 1..=8 {
            let done = advance_route(
                &graph,
                |_, _| false,
                |_, _| false,
                &agent,
                LocomotionMode::Ground,
                None,
                from,
                to,
                &mut route,
                &mut scratch,
                &mut search,
                2,
                1000,
            )
            .unwrap();
            assert_eq!(done, slice == 8);
            if !done {
                assert!(route.points.is_empty());
            }
        }
        assert_eq!(search.expansions, 15);
        assert_eq!(route.points.last(), Some(&to));
    }
}

#[test]
fn a_route_never_turns_back_to_its_start_or_goal_cell_centre() {
    let nodes = vec![
        TerrainDerivedNavigationGraphNode {
            height_centimetres: 0,
            minimum_triangle_normal_y: 1.0,
            clearance_centimetres: 100,
            water_depth_centimetres: 0,
            eligibility_flags: NavFlags::GUEST,
        };
        16
    ];
    let graph =
        TerrainDerivedNavigationGraph::from_terrain_nodes(100, 4, [1, 1], nodes.into_boxed_slice());
    let agent = NavAgent {
        radius_m: 0.25,
        max_speed_mps: 1.0,
        acceleration_mps2: 1.0,
        capabilities: NavFlags::GUEST,
    };
    let mut scratch = RouteScratch::default();
    scratch.ensure_capacity(16, 17);
    let mut route = Route::with_capacity(17);
    let (start, destination) = (Vec3::new(0.8, 0.0, -0.5), Vec3::new(3.2, 0.0, -0.5));
    plan_route(
        &graph,
        |_, _| false,
        |_, _| false,
        &agent,
        LocomotionMode::Ground,
        None,
        start,
        destination,
        &mut route,
        &mut scratch,
        crate::locomotion::pathfinding::LONG_PATH_ITERATIONS,
    )
    .unwrap();
    let mut previous = start;
    for point in &route.points {
        assert!(point.x >= previous.x, "{start:?} -> {:?}", route.points);
        previous = *point;
    }
    assert_eq!(route.points.last(), Some(&destination));
}
