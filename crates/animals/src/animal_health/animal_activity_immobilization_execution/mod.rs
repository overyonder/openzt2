use bevy::prelude::*;

use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::Docking;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::Steering;

use super::types::{Dead, FreezeAnimal, Frozen, ThawAnimal};

type AnimalActivityStateRemovedWhileFrozen = (
    BehaviorTaskExecutionState,
    PendingBehaviorAnimationClipCompletion,
    Destination,
    Route,
    Steering,
    Docking,
);

pub(super) fn apply_animal_freeze_and_thaw_requests(
    mut commands: Commands,
    mut freeze_animal_requests: MessageReader<FreezeAnimal>,
    mut thaw_animal_requests: MessageReader<ThawAnimal>,
    live_animals: Query<(), (With<Animal>, Without<Dead>)>,
) {
    for request in freeze_animal_requests.read() {
        if live_animals.get(request.animal).is_err() {
            continue;
        }
        commands
            .entity(request.animal)
            .insert(Frozen)
            .remove::<AnimalActivityStateRemovedWhileFrozen>();
    }
    for request in thaw_animal_requests.read() {
        if live_animals.get(request.animal).is_ok() {
            commands.entity(request.animal).remove::<Frozen>();
        }
    }
}

pub(super) fn remove_new_activity_state_from_frozen_animals(
    mut commands: Commands,
    frozen_animals: Query<Entity, (With<Animal>, With<Frozen>, Without<Dead>)>,
) {
    for animal in &frozen_animals {
        commands
            .entity(animal)
            .remove::<AnimalActivityStateRemovedWhileFrozen>();
    }
}
