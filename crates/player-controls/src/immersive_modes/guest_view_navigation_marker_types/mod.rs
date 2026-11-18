use bevy::prelude::*;

#[derive(Component)]
#[relationship(relationship_target = GuestViewNavigationMarkers)]
pub(super) struct GuestViewNavigationMarkerOf(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = GuestViewNavigationMarkerOf, linked_spawn)]
pub(super) struct GuestViewNavigationMarkers(Vec<Entity>);

#[derive(Component)]
pub(super) struct GuestViewNavigationMarkerPresentation {
    pub(super) targeted_entity: Entity,
}
