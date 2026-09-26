use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::{
    topology_graph_types::{EdgeKey, FenceEdge, TopologyIndex, TopologyNode},
    topology_index_hydration::index_loaded_topology_and_remove_duplicate_or_invalid_entities,
};

#[test]
fn index_load_canonicalizes_and_rejects_duplicate_entries() {
    let mut app = App::new();
    app.init_resource::<TopologyIndex>().add_systems(
        Update,
        index_loaded_topology_and_remove_duplicate_or_invalid_entities,
    );
    let a = app
        .world_mut()
        .spawn(TopologyNode { cell: IVec3::ZERO })
        .id();
    let b = app.world_mut().spawn(TopologyNode { cell: IVec3::X }).id();
    let duplicate = app
        .world_mut()
        .spawn(TopologyNode { cell: IVec3::ZERO })
        .id();
    let edge = app
        .world_mut()
        .spawn(FenceEdge {
            definition: AssetId::default(),
            a,
            b,
        })
        .id();
    app.update();
    let index = app.world().resource::<TopologyIndex>();
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
