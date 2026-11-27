use super::*;
use crate::habitat::habitat_types::HabitatRegion;
use crate::locomotion::terrain_derived_navigation_graph_types::{
    NavigationTerrainCellEdit, TerrainDerivedNavigationGraph, TerrainDerivedNavigationGraphNode,
    TerrainDerivedNavigationGraphTraversalKind,
};
use crate::topology::topology_graph_types::TopologyGrid;
use openzt2_game_data::CaffeineFactors;

#[test]
fn shore_candidates_respect_water_clearance_capabilities_and_terrain_changes() {
    let node = TerrainDerivedNavigationGraphNode {
        height_centimetres: 0,
        minimum_triangle_normal_y: 1.0,
        clearance_centimetres: 100,
        water_depth_centimetres: 0,
        eligibility_flags: NavFlags::STAFF,
    };
    let mut nodes = vec![node; 16];
    nodes[0].eligibility_flags.0 |= NavFlags::WATER.0;
    nodes[4].clearance_centimetres = 10;
    nodes[8].eligibility_flags = NavFlags::GUEST;
    let mut navigation = ActiveTerrainDerivedNavigationGraph::new(
        TerrainDerivedNavigationGraph::from_terrain_nodes(100, 4, [1, 1], nodes.into_boxed_slice()),
    );
    let region = HabitatRegion {
        topology_cells: [
            IVec2::new(3, -1),
            IVec2::new(2, -1),
            IVec2::new(1, -1),
            IVec2::new(0, -1),
        ]
        .into(),
        area_square_metres: 4.0,
    };
    let agent = NavAgent {
        radius_m: 0.5,
        max_speed_mps: 1.0,
        acceleration_mps2: 1.0,
        capabilities: NavFlags::STAFF,
    };
    let mut candidates = Vec::new();
    navigation.collect_dry_ground_candidates(
        None,
        &region,
        TopologyGrid::default(),
        &agent,
        Vec3::new(0.5, 0.0, -0.5),
        &mut candidates,
    );
    assert_eq!(candidates, [Vec3::new(3.5, 0.0, -0.5)]);
    let graph = navigation.terrain_derived_navigation_graph_mut();
    let cell = UVec2::new(3, 0);
    let changed_node = graph
        .calculate_navigation_node_for_terrain_cell(cell.to_array())
        .unwrap();
    let changes = graph.apply_terrain_cell_edits(
        [NavigationTerrainCellEdit {
            cell,
            height_centimetres: 0,
            minimum_triangle_normal_y: 1.0,
            contains_water: true,
            water_depth_centimetres: 100,
        }],
        None,
    );
    assert_eq!(
        changes.affected_nodes[&changed_node][1].water_depth_centimetres,
        100
    );
    assert_eq!(changes.affected_edges.len(), 4);
    for (edge, change) in changes.affected_edges {
        assert!(edge.contains(&changed_node));
        assert_eq!(
            change.before.unwrap().traversal_kind,
            TerrainDerivedNavigationGraphTraversalKind::Ground
        );
        assert_eq!(
            change.after.unwrap().traversal_kind,
            TerrainDerivedNavigationGraphTraversalKind::Shore
        );
    }
    navigation.collect_dry_ground_candidates(
        None,
        &region,
        TopologyGrid::default(),
        &agent,
        Vec3::new(0.5, 0.0, -0.5),
        &mut candidates,
    );
    assert!(candidates.is_empty());
}

#[test]
fn caffeine_flags_use_both_authored_factors_in_order() {
    let mut flags = CaffeineFlags {
        caffeinated: true,
        decaffeinated: true,
    };
    let factors = CaffeineFactors {
        caffeinated: 1.0e30,
        decaffeinated: 1.0e30,
    };
    assert!(flags.apply_to_rate(1.0e30, factors).is_infinite());

    flags.caffeinated = false;
    assert_eq!(
        flags.apply_to_rate(
            12.0,
            CaffeineFactors {
                caffeinated: 3.0,
                decaffeinated: 6.0,
            },
        ),
        2.0,
    );
    assert_eq!(flags.apply_to_rate(12.0, CaffeineFactors::default()), 12.0);
}
