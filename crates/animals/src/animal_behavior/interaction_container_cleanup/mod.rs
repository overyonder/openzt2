use super::behavior_object_attachment_execution::ContainedObject;
use super::interaction_container_occupancy::InteractionContainerOccupancy;
use bevy::prelude::*;

pub(super) fn remove_destroyed_container_memberships(
    mut occupancy: ResMut<InteractionContainerOccupancy>,
    entities: Query<()>,
    contained: Query<(), With<ContainedObject>>,
) {
    occupancy.remove_destroyed_entities(&entities, |member| contained.contains(member));
}
