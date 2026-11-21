use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
};

use super::topology_edit_types::{FenceConnectivityChanged, PathConnectivityChanged};
use super::topology_graph_types::{
    EdgeKey, FenceEnd, FenceStart, PathTile, TopologyIndex, TopologyNode,
};
use super::topology_grid_geometry::calculate_adjacent_topology_grid_cells_between_endpoints;

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
            index
                .path_columns
                .entry(path.cell.xy())
                .or_default()
                .push(context.entity);
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
            if let bevy::platform::collections::hash_map::Entry::Occupied(mut column) =
                index.path_columns.entry(path.cell.xy())
            {
                column.get_mut().retain(|&entity| entity != context.entity);
                if column.get().is_empty() {
                    column.remove();
                }
            }
            if let Some(mut changes) = world.get_resource_mut::<Messages<PathConnectivityChanged>>()
            {
                changes.write(PathConnectivityChanged(path.cell));
            }
        }
    }
}

pub(super) fn index_inserted_fence_edge(mut world: DeferredWorld, context: HookContext) {
    let (Some(start), Some(end)) = (
        world.get::<FenceStart>(context.entity).copied(),
        world.get::<FenceEnd>(context.entity).copied(),
    ) else {
        return;
    };
    let (Some(first), Some(second)) = (
        world.get::<TopologyNode>(start.0),
        world.get::<TopologyNode>(end.0),
    ) else {
        return;
    };
    let Some(key) = EdgeKey::new(first.cell, second.cell) else {
        return;
    };
    let canonical_nodes = world.get_resource::<TopologyIndex>().is_some_and(|index| {
        index.nodes.get(&first.cell) == Some(&start.0)
            && index.nodes.get(&second.cell) == Some(&end.0)
    });
    if !canonical_nodes {
        return;
    }
    let mut index = world.resource_mut::<TopologyIndex>();
    if let bevy::platform::collections::hash_map::Entry::Vacant(slot) = index.edges.entry(key) {
        slot.insert(context.entity);
        index.edge_keys.insert(context.entity, key);
        for cell in fence_columns(key) {
            index
                .fence_columns
                .entry(cell)
                .or_default()
                .push(context.entity);
        }
        for step in fine_steps(key) {
            index
                .fine_fence_steps
                .entry(step)
                .or_default()
                .push(context.entity);
        }
        if let Some(mut changes) = world.get_resource_mut::<Messages<FenceConnectivityChanged>>() {
            changes.write(FenceConnectivityChanged(key));
        }
    }
}

pub(super) fn discard_indexed_fence_edge(mut world: DeferredWorld, context: HookContext) {
    if let Some(mut index) = world.get_resource_mut::<TopologyIndex>() {
        if let Some(key) = index.edge_keys.remove(&context.entity) {
            index.edges.remove(&key);
            for cell in fence_columns(key) {
                remove_from_bucket(&mut index.fence_columns, cell, context.entity);
            }
            for step in fine_steps(key) {
                remove_from_bucket(&mut index.fine_fence_steps, step, context.entity);
            }
            if let Some(mut changes) =
                world.get_resource_mut::<Messages<FenceConnectivityChanged>>()
            {
                changes.write(FenceConnectivityChanged(key));
            }
        }
    }
}

fn fence_columns(key: EdgeKey) -> impl Iterator<Item = IVec2> {
    let min = key.lo.xy().min(key.hi.xy());
    let max = key.lo.xy().max(key.hi.xy());
    (min.y..=max.y).flat_map(move |y| (min.x..=max.x).map(move |x| IVec2::new(x, y)))
}

fn fine_steps(key: EdgeKey) -> impl Iterator<Item = EdgeKey> {
    let cells = calculate_adjacent_topology_grid_cells_between_endpoints(key.lo, key.hi)
        .unwrap_or_default();
    (1..cells.len()).filter_map(move |index| EdgeKey::new(cells[index - 1], cells[index]))
}

fn remove_from_bucket<K: Eq + std::hash::Hash>(
    buckets: &mut bevy::platform::collections::HashMap<K, Vec<Entity>>,
    key: K,
    entity: Entity,
) {
    if let bevy::platform::collections::hash_map::Entry::Occupied(mut bucket) = buckets.entry(key) {
        bucket.get_mut().retain(|&candidate| candidate != entity);
        if bucket.get().is_empty() {
            bucket.remove();
        }
    }
}
