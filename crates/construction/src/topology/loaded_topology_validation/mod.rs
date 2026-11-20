use bevy::prelude::*;

use openzt2_zoo_state::topology::topology_graph_types::{
    EdgeKey, FenceEdge, PathTile, TopologyIndex, TopologyNode,
};

pub(super) fn remove_duplicate_or_invalid_loaded_topology(
    mut commands: Commands,
    index: Res<TopologyIndex>,
    nodes: Query<(Entity, &TopologyNode)>,
    paths: Query<(Entity, &PathTile)>,
    edges: Query<(Entity, &FenceEdge)>,
) {
    for (entity, node) in &nodes {
        if index.nodes.get(&node.cell) != Some(&entity) {
            commands.entity(entity).despawn();
        }
    }
    for (entity, path) in &paths {
        if index.paths.get(&path.cell) != Some(&entity) {
            commands.entity(entity).despawn();
        }
    }
    for (entity, edge) in &edges {
        let key = nodes.get(edge.a).ok().zip(nodes.get(edge.b).ok()).and_then(
            |((first_entity, first), (second_entity, second))| {
                if index.nodes.get(&first.cell) != Some(&first_entity)
                    || index.nodes.get(&second.cell) != Some(&second_entity)
                {
                    return None;
                }
                EdgeKey::new(first.cell, second.cell)
            },
        );
        if key.and_then(|key| index.edges.get(&key)) != Some(&entity) {
            commands.entity(entity).despawn();
        }
    }
}
