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

#[allow(clippy::type_complexity)]
pub(super) fn release_people_whose_task_moved_on(
    mut occupancy: ResMut<InteractionContainerOccupancy>,
    people: Query<
        (
            Entity,
            Option<&openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState>,
        ),
        Or<(
            With<openzt2_zoo_state::guests::guest_simulation_types::Guest>,
            With<openzt2_zoo_state::staff::staff_employment_types::Staff>,
        )>,
    >,
) {
    // Only the members themselves, not every person in the zoo.
    let moved_on = occupancy
        .memberships()
        .filter_map(|(member, _)| {
            let (_, task) = people.get(member).ok()?;
            Some((member, task.and_then(|task| task.target())))
        })
        .collect::<Vec<_>>();
    for (person, target) in moved_on {
        occupancy.release_after_task_moved_on(person, target);
    }
}
