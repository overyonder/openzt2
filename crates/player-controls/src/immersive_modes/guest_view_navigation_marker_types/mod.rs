use bevy::prelude::*;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_data::AssetId;

#[derive(Component)]
pub(super) struct GuestViewProjection {
    pub(super) excluded_subject: Option<Entity>,
    pub(super) cursor: Option<(AssetId, Handle<ScenePrefabAsset>)>,
    pub(super) marker: Option<(AssetId, Handle<ScenePrefabAsset>)>,
}

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
    pub(super) translation_offset: Vec3,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum GuestViewMarkerSlot {
    Cursor,
    Position(usize),
}
