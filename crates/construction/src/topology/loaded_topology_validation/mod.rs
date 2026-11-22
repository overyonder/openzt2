use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::topology::topology_graph_types::FenceSegment;

use openzt2_zoo_state::topology::topology_graph_types::{
    EdgeKey, PathTile, TopologyIndex, TopologyNode,
};

pub(super) fn remove_duplicate_or_invalid_loaded_topology(
    mut commands: Commands,
    indexes: Query<&TopologyIndex>,
    nodes: Query<(Entity, &TopologyNode, &WorldMember)>,
    paths: Query<(Entity, &PathTile, &WorldMember)>,
    edges: Query<(Entity, FenceSegment, &WorldMember)>,
) {
    for (entity, node, member) in &nodes {
        if indexes
            .get(member.root)
            .ok()
            .and_then(|index| index.nodes.get(&node.cell))
            != Some(&entity)
        {
            commands.entity(entity).despawn();
        }
    }
    for (entity, path, member) in &paths {
        if indexes
            .get(member.root)
            .ok()
            .and_then(|index| index.paths.get(&path.cell))
            != Some(&entity)
        {
            commands.entity(entity).despawn();
        }
    }
    for (entity, edge, member) in &edges {
        let Ok(index) = indexes.get(member.root) else {
            continue;
        };
        let key = nodes
            .get(edge.a())
            .ok()
            .zip(nodes.get(edge.b()).ok())
            .and_then(
                |((first_entity, first, first_member), (second_entity, second, second_member))| {
                    if first_member.root != member.root
                        || second_member.root != member.root
                        || index.nodes.get(&first.cell) != Some(&first_entity)
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
