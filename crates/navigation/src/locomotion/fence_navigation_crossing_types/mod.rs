use bevy::{
    ecs::{lifecycle::HookContext, system::SystemParam, world::DeferredWorld},
    platform::collections::HashMap,
    prelude::*,
};

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

pub(super) type DirectedNavigationEdge = (u32, u32);

#[derive(Component)]
#[component(immutable, on_insert = index_edge, on_discard = unindex_edge)]
pub(super) struct NavigationEdgeKey(pub(super) DirectedNavigationEdge);

/// Compact numeric graph edges get an ECS entity only when crossed by a fence.
#[derive(Component, Default)]
pub(super) struct FenceNavigationCrossingIndex(pub(super) HashMap<DirectedNavigationEdge, Entity>);

#[derive(Component)]
#[relationship(relationship_target = FenceCrossings)]
pub(super) struct CrossingOfFence(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = CrossingOfFence, linked_spawn)]
pub(super) struct FenceCrossings(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = EdgeCrossings)]
pub(super) struct CrossingOfEdge(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = CrossingOfEdge, linked_spawn)]
pub(super) struct EdgeCrossings(Vec<Entity>);

#[derive(SystemParam)]
pub(super) struct FenceNavigationCrossingQueries<'w, 's> {
    indexes: Query<'w, 's, Ref<'static, FenceNavigationCrossingIndex>>,
    edges: Query<'w, 's, &'static EdgeCrossings>,
    crossings: Query<'w, 's, &'static CrossingOfFence>,
}

impl FenceNavigationCrossingQueries<'_, '_> {
    pub(super) fn is_changed(&self, root: Entity) -> bool {
        self.indexes.get(root).is_ok_and(|index| index.is_changed())
    }

    pub(super) fn crossing_fences(
        &self,
        root: Entity,
        from: u32,
        to: u32,
    ) -> impl Iterator<Item = Entity> + '_ {
        self.indexes
            .get(root)
            .ok()
            .and_then(|index| index.0.get(&(from, to)).copied())
            .and_then(|edge| self.edges.get(edge).ok())
            .into_iter()
            .flat_map(|links| links.iter())
            .filter_map(|link| self.crossings.get(link).ok().map(|fence| fence.0))
    }
}

fn index_edge(mut world: DeferredWorld, context: HookContext) {
    let entity = context.entity;
    let Some((key, root)) = world
        .get::<NavigationEdgeKey>(entity)
        .zip(world.get::<WorldMember>(entity))
        .map(|(key, member)| (key.0, member.root))
    else {
        return;
    };
    if let Some(mut index) = world.get_mut::<FenceNavigationCrossingIndex>(root) {
        index.0.insert(key, entity);
    }
}

fn unindex_edge(mut world: DeferredWorld, context: HookContext) {
    let entity = context.entity;
    let Some((key, root)) = world
        .get::<NavigationEdgeKey>(entity)
        .zip(world.get::<WorldMember>(entity))
        .map(|(key, member)| (key.0, member.root))
    else {
        return;
    };
    if let Some(mut index) = world.get_mut::<FenceNavigationCrossingIndex>(root) {
        if index.0.get(&key) == Some(&entity) {
            index.0.remove(&key);
        }
    }
}

pub(super) fn prune_empty_route_edges(
    mut commands: Commands,
    mut removed: RemovedComponents<EdgeCrossings>,
    empty: Query<(), (With<NavigationEdgeKey>, Without<EdgeCrossings>)>,
) {
    for entity in removed.read().filter(|&entity| empty.contains(entity)) {
        commands.entity(entity).try_despawn();
    }
}

pub(super) fn mark_crossing_inserted(
    event: On<Insert<CrossingOfEdge>>,
    crossings: Query<&CrossingOfEdge>,
    edges: Query<&WorldMember, With<NavigationEdgeKey>>,
    mut indexes: Query<&mut FenceNavigationCrossingIndex>,
) {
    if let Some(mut index) = crossings
        .get(event.entity)
        .ok()
        .and_then(|crossing| edges.get(crossing.0).ok())
        .and_then(|member| indexes.get_mut(member.root).ok())
    {
        index.set_changed();
    }
}

pub(super) fn mark_crossing_discarded(
    event: On<Discard<CrossingOfEdge>>,
    crossings: Query<&CrossingOfEdge>,
    edges: Query<&WorldMember, With<NavigationEdgeKey>>,
    mut indexes: Query<&mut FenceNavigationCrossingIndex>,
) {
    if let Some(mut index) = crossings
        .get(event.entity)
        .ok()
        .and_then(|crossing| edges.get(crossing.0).ok())
        .and_then(|member| indexes.get_mut(member.root).ok())
    {
        index.set_changed();
    }
}
