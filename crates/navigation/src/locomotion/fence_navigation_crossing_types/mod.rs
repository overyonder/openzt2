use bevy::platform::collections::HashMap;
use bevy::prelude::*;

pub(super) type DirectedNavigationEdge = (u32, u32);

#[derive(Component)]
#[component(immutable)]
pub(super) struct FenceNavigationCrossings(pub(super) Vec<DirectedNavigationEdge>);

/// Reverse lookup for route searches. Each fence owns its crossed edge keys.
#[derive(Resource, Default)]
pub(super) struct FenceNavigationCrossingIndex {
    fences_by_edge: HashMap<DirectedNavigationEdge, Vec<Entity>>,
}

impl FenceNavigationCrossingIndex {
    pub(super) fn crossing_fences(&self, from: u32, to: u32) -> &[Entity] {
        self.fences_by_edge
            .get(&(from, to))
            .map_or(&[], Vec::as_slice)
    }
}

pub(super) fn index_inserted_fence_crossings(
    insert: On<Insert<FenceNavigationCrossings>>,
    crossings: Query<&FenceNavigationCrossings>,
    mut index: ResMut<FenceNavigationCrossingIndex>,
) {
    let Ok(crossings) = crossings.get(insert.entity) else {
        return;
    };
    for &edge in &crossings.0 {
        let fences = index.fences_by_edge.entry(edge).or_default();
        if let Err(position) = fences.binary_search(&insert.entity) {
            fences.insert(position, insert.entity);
        }
    }
}

pub(super) fn unindex_discarded_fence_crossings(
    discard: On<Discard<FenceNavigationCrossings>>,
    crossings: Query<&FenceNavigationCrossings>,
    mut index: ResMut<FenceNavigationCrossingIndex>,
) {
    let Ok(crossings) = crossings.get(discard.entity) else {
        return;
    };
    for edge in &crossings.0 {
        let Some(fences) = index.fences_by_edge.get_mut(edge) else {
            continue;
        };
        if let Ok(position) = fences.binary_search(&discard.entity) {
            fences.remove(position);
            if fences.is_empty() {
                index.fences_by_edge.remove(edge);
            }
        }
    }
}
