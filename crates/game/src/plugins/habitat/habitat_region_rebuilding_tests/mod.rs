use bevy::prelude::*;

use super::{
    habitat_region_rebuilding::request_initial_habitat_region_rebuild_from_loaded_topology,
    habitat_types::RebuildHabitatRegionsWithinTopologyBounds,
};

#[test]
fn loaded_topology_requests_one_initial_habitat_region_rebuild() {
    let mut application = App::new();
    application
        .add_message::<RebuildHabitatRegionsWithinTopologyBounds>()
        .add_systems(
            Update,
            request_initial_habitat_region_rebuild_from_loaded_topology,
        );
    application.world_mut().spawn(
        crate::plugins::topology::topology_graph_types::TopologyNode {
            cell: IVec3::new(-2, 0, 4),
        },
    );
    application.world_mut().spawn(
        crate::plugins::topology::topology_graph_types::TopologyNode {
            cell: IVec3::new(5, 0, -3),
        },
    );
    application.update();
    assert_eq!(
        application
            .world()
            .resource::<Messages<RebuildHabitatRegionsWithinTopologyBounds>>()
            .len(),
        1,
    );
}
