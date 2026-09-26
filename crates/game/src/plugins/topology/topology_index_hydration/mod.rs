use bevy::{platform::collections::HashSet, prelude::*};

use super::topology_graph_types::{EdgeKey, FenceEdge, PathTile, TopologyIndex, TopologyNode};

pub(super) fn index_loaded_topology_and_remove_duplicate_or_invalid_entities(
    mut commands: Commands,
    mut topology_index: ResMut<TopologyIndex>,
    nodes: Query<(Entity, &TopologyNode)>,
    paths: Query<(Entity, &PathTile)>,
    edges: Query<(Entity, &FenceEdge)>,
) {
    topology_index.nodes.clear();
    topology_index.paths.clear();
    topology_index.edges.clear();
    topology_index.nodes.reserve(nodes.iter().len());
    topology_index.paths.reserve(paths.iter().len());
    topology_index.edges.reserve(edges.iter().len());

    let mut rejected_nodes = HashSet::new();
    for (node_entity, node) in &nodes {
        if topology_index.nodes.contains_key(&node.cell) {
            rejected_nodes.insert(node_entity);
            commands.entity(node_entity).despawn();
        } else {
            topology_index.nodes.insert(node.cell, node_entity);
        }
    }
    for (path_entity, path) in &paths {
        if topology_index.paths.contains_key(&path.cell) {
            commands.entity(path_entity).despawn();
        } else {
            topology_index.paths.insert(path.cell, path_entity);
        }
    }
    for (edge_entity, edge) in &edges {
        if rejected_nodes.contains(&edge.a) || rejected_nodes.contains(&edge.b) {
            commands.entity(edge_entity).despawn();
            continue;
        }
        let Ok((_, first_node)) = nodes.get(edge.a) else {
            commands.entity(edge_entity).despawn();
            continue;
        };
        let Ok((_, second_node)) = nodes.get(edge.b) else {
            commands.entity(edge_entity).despawn();
            continue;
        };
        let Some(edge_key) = EdgeKey::new(first_node.cell, second_node.cell) else {
            commands.entity(edge_entity).despawn();
            continue;
        };
        if topology_index.edges.contains_key(&edge_key) {
            commands.entity(edge_entity).despawn();
        } else {
            topology_index.edges.insert(edge_key, edge_entity);
        }
    }
}
