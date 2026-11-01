use super::*;
use crate::habitat::habitat_types::HabitatRegion;
use crate::locomotion::terrain_derived_navigation_graph_types::{
    TerrainDerivedNavigationGraph, TerrainDerivedNavigationGraphNode,
};
use crate::topology::topology_graph_types::TopologyGrid;

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
        boundary_fence_entities: Box::default(),
    };
    let agent = NavAgent {
        radius_m: 0.5,
        max_speed_mps: 1.0,
        acceleration_mps2: 1.0,
        capabilities: NavFlags::STAFF,
    };
    let mut candidates = Vec::new();
    navigation.collect_dry_ground_candidates(
        &region,
        TopologyGrid::default(),
        &agent,
        Vec3::new(0.5, 0.0, -0.5),
        &mut candidates,
    );
    assert_eq!(candidates, [Vec3::new(3.5, 0.0, -0.5)]);
    navigation
        .terrain_derived_navigation_graph_mut()
        .update_navigation_node_from_changed_terrain_cell(UVec2::new(3, 0), 0, true, 1.0)
        .unwrap();
    navigation.collect_dry_ground_candidates(
        &region,
        TopologyGrid::default(),
        &agent,
        Vec3::new(0.5, 0.0, -0.5),
        &mut candidates,
    );
    assert!(candidates.is_empty());
}

#[test]
fn clearing_one_caffeine_mode_keeps_the_other() {
    let mut world = World::new();
    let actor = world.spawn_empty().id();
    let mut write = |mode, enabled| {
        let current = world.get::<CaffeineMode>(actor).copied();
        let mut commands = world.commands();
        CaffeineMode::write_authored(&mut commands.entity(actor), current.as_ref(), mode, enabled);
        world.flush();
        world.get::<CaffeineMode>(actor).copied()
    };
    assert_eq!(
        write(CaffeineMode::Caffeinated, true),
        Some(CaffeineMode::Caffeinated)
    );
    assert_eq!(
        write(CaffeineMode::Decaffeinated, false),
        Some(CaffeineMode::Caffeinated)
    );
    assert_eq!(write(CaffeineMode::Caffeinated, false), None);
    assert_eq!(CaffeineMode::speed_factor(None), 1.0);
    assert_eq!(
        CaffeineMode::speed_factor(Some(&CaffeineMode::Decaffeinated)),
        1.0 / 3.0
    );
}
