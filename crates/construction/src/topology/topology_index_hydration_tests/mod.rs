use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

use crate::topology::loaded_topology_validation::remove_duplicate_or_invalid_loaded_topology;
use openzt2_zoo_state::topology::topology_graph_types::EdgeKey;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;

#[test]
fn index_load_canonicalizes_and_rejects_duplicate_entries() {
    let mut app = App::new();
    app.add_systems(Update, remove_duplicate_or_invalid_loaded_topology);
    let root = app
        .world_mut()
        .spawn((
            WorldRoot {
                scenario: AssetId::default(),
            },
            TopologyIndex::default(),
        ))
        .id();
    let a = app
        .world_mut()
        .spawn((TopologyNode { cell: IVec3::ZERO }, WorldMember { root }))
        .id();
    let b = app
        .world_mut()
        .spawn((TopologyNode { cell: IVec3::X }, WorldMember { root }))
        .id();
    let duplicate = app
        .world_mut()
        .spawn((TopologyNode { cell: IVec3::ZERO }, WorldMember { root }))
        .id();
    let edge = app
        .world_mut()
        .spawn((
            FenceEdge::between(AssetId::default(), a, b),
            WorldMember { root },
        ))
        .id();
    app.update();
    let index = app.world().get::<TopologyIndex>(root).unwrap();
    assert_eq!(index.nodes.len(), 2);
    assert!(matches!(index.nodes[&IVec3::ZERO], entity if entity == a || entity == duplicate));
    assert_eq!(
        index.edges[&EdgeKey::new(IVec3::ZERO, IVec3::X).unwrap()],
        edge
    );
    assert_ne!(
        app.world().get_entity(a).is_ok(),
        app.world().get_entity(duplicate).is_ok()
    );
}
