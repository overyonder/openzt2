use super::*;
use openzt2_zoo_state::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraphNode;

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
    let mut graph = TerrainDerivedNavigationGraph {
        terrain_cell_size_centimetres: 100,
        terrain_cells_per_tile: 4,
        terrain_tile_size_centimetres: 400,
        terrain_tile_grid_dimensions: [1, 1],
        navigation_nodes: nodes.into_boxed_slice(),
    };
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
