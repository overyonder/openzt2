use bevy::prelude::*;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;

#[derive(Component)]
#[relationship(relationship_target = GuestViewNavigationMarkers)]
pub(super) struct GuestViewNavigationMarkerOf(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = GuestViewNavigationMarkerOf, linked_spawn)]
pub(super) struct GuestViewNavigationMarkers(Vec<Entity>);

#[derive(Component)]
pub(super) struct GuestViewNavigationMarker {
    pub(super) slot: GuestViewMarkerSlot,
    pub(super) prefab: Handle<ScenePrefabAsset>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum GuestViewMarkerSlot {
    Cursor,
    Position(usize),
}
