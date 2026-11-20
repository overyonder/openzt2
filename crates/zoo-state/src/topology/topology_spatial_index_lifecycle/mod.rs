use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
};

use super::topology_edit_types::PathConnectivityChanged;
use super::topology_graph_types::{EdgeKey, FenceEdge, PathTile, TopologyIndex, TopologyNode};

pub(super) fn index_inserted_topology_node(mut world: DeferredWorld, context: HookContext) {
    let Some(node) = world.get::<TopologyNode>(context.entity).copied() else {
        return;
    };
    if let Some(mut index) = world.get_resource_mut::<TopologyIndex>() {
        index.nodes.entry(node.cell).or_insert(context.entity);
    }
}

pub(super) fn discard_indexed_topology_node(mut world: DeferredWorld, context: HookContext) {
    let Some(node) = world.get::<TopologyNode>(context.entity).copied() else {
        return;
    };
    if let Some(mut index) = world.get_resource_mut::<TopologyIndex>() {
        if index.nodes.get(&node.cell) == Some(&context.entity) {
            index.nodes.remove(&node.cell);
        }
    }
}

pub(super) fn index_inserted_path_tile(mut world: DeferredWorld, context: HookContext) {
    let Some(path) = world.get::<PathTile>(context.entity).copied() else {
        return;
    };
    if let Some(mut index) = world.get_resource_mut::<TopologyIndex>() {
        if let bevy::platform::collections::hash_map::Entry::Vacant(slot) =
            index.paths.entry(path.cell)
        {
            slot.insert(context.entity);
            if let Some(mut changes) = world.get_resource_mut::<Messages<PathConnectivityChanged>>()
            {
                changes.write(PathConnectivityChanged(path.cell));
            }
        }
    }
}

pub(super) fn discard_indexed_path_tile(mut world: DeferredWorld, context: HookContext) {
    let Some(path) = world.get::<PathTile>(context.entity).copied() else {
        return;
    };
    if let Some(mut index) = world.get_resource_mut::<TopologyIndex>() {
        if index.paths.get(&path.cell) == Some(&context.entity) {
            index.paths.remove(&path.cell);
            if let Some(mut changes) = world.get_resource_mut::<Messages<PathConnectivityChanged>>()
            {
                changes.write(PathConnectivityChanged(path.cell));
            }
        }
    }
}

pub(super) fn index_inserted_fence_edge(mut world: DeferredWorld, context: HookContext) {
    let Some(edge) = world.get::<FenceEdge>(context.entity) else {
        return;
    };
    let (Some(first), Some(second)) = (
        world.get::<TopologyNode>(edge.a),
        world.get::<TopologyNode>(edge.b),
    ) else {
        return;
    };
    let Some(key) = EdgeKey::new(first.cell, second.cell) else {
        return;
    };
    let canonical_nodes = world.get_resource::<TopologyIndex>().is_some_and(|index| {
        index.nodes.get(&first.cell) == Some(&edge.a)
            && index.nodes.get(&second.cell) == Some(&edge.b)
    });
    if !canonical_nodes {
        return;
    }
    let mut index = world.resource_mut::<TopologyIndex>();
    if let bevy::platform::collections::hash_map::Entry::Vacant(slot) = index.edges.entry(key) {
        slot.insert(context.entity);
        index.edge_keys.insert(context.entity, key);
    }
}

pub(super) fn discard_indexed_fence_edge(mut world: DeferredWorld, context: HookContext) {
    if let Some(mut index) = world.get_resource_mut::<TopologyIndex>() {
        if let Some(key) = index.edge_keys.remove(&context.entity) {
            index.edges.remove(&key);
        }
    }
}
