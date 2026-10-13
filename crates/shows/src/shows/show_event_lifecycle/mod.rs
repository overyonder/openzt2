use bevy::prelude::*;

use super::show_event_types::AuthoredAnimalShowEvent;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;

pub(crate) fn remove_expired_animal_show_events(
    mut commands: Commands,
    events: Query<(Entity, &WorldMember, &AuthoredAnimalShowEvent)>,
    performers: Query<(&WorldMember, &BehaviorTaskExecutionState), With<Animal>>,
    interruptions: Query<(), Or<(With<PendingBehaviorTaskFailure>, With<PendingBehaviorSet>)>>,
) {
    for (entity, world, event) in &events {
        if event.policy.expire_with_task
            && (interruptions.contains(event.performer)
                || !performers.get(event.performer).is_ok_and(|(owner, task)| {
                    owner.root == world.root
                        && task.execution_id == event.execution_id
                        && task.phase != BehaviorTaskExecutionPhase::Failure
                }))
        {
            commands.entity(entity).despawn();
        }
    }
}
