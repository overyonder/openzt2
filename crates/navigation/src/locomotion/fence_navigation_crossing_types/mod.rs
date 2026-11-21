use bevy::{
    ecs::{lifecycle::HookContext, system::SystemParam, world::DeferredWorld},
    platform::collections::HashMap,
    prelude::*,
};

pub(super) type DirectedNavigationEdge = (u32, u32);

#[derive(Component)]
#[component(immutable, on_insert = index_edge, on_discard = unindex_edge)]
pub(super) struct NavigationEdgeKey(pub(super) DirectedNavigationEdge);

/// Compact numeric graph edges get an ECS entity only when crossed by a fence.
#[derive(Resource, Default)]
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
    index: Res<'w, FenceNavigationCrossingIndex>,
    edges: Query<'w, 's, &'static EdgeCrossings>,
    crossings: Query<'w, 's, &'static CrossingOfFence>,
}

impl FenceNavigationCrossingQueries<'_, '_> {
    pub(super) fn is_changed(&self) -> bool {
        self.index.is_changed()
    }

    pub(super) fn crossing_fences(&self, from: u32, to: u32) -> impl Iterator<Item = Entity> + '_ {
        self.index
            .0
            .get(&(from, to))
            .and_then(|&edge| self.edges.get(edge).ok())
            .into_iter()
            .flat_map(|links| links.iter())
            .filter_map(|link| self.crossings.get(link).ok().map(|fence| fence.0))
    }
}

fn index_edge(mut world: DeferredWorld, context: HookContext) {
    let Some(key) = world
        .get::<NavigationEdgeKey>(context.entity)
        .map(|key| key.0)
    else {
        return;
    };
    world
        .resource_mut::<FenceNavigationCrossingIndex>()
        .0
        .insert(key, context.entity);
}

fn unindex_edge(mut world: DeferredWorld, context: HookContext) {
    let Some(key) = world
        .get::<NavigationEdgeKey>(context.entity)
        .map(|key| key.0)
    else {
        return;
    };
    let mut index = world.resource_mut::<FenceNavigationCrossingIndex>();
    if index.0.get(&key) == Some(&context.entity) {
        index.0.remove(&key);
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

pub(super) fn mark_crossing_added(
    _event: On<Add<CrossingOfFence>>,
    mut index: ResMut<FenceNavigationCrossingIndex>,
) {
    index.set_changed();
}

pub(super) fn mark_crossing_removed(
    _event: On<Remove<CrossingOfFence>>,
    mut index: ResMut<FenceNavigationCrossingIndex>,
) {
    index.set_changed();
}
