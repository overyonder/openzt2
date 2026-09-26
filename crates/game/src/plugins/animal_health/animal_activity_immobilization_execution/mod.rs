use bevy::prelude::*;

use crate::plugins::{
    animal_lifecycle::types::Animal,
    behavior_task_execution_types::{
        BehaviorTaskExecutionState, PendingBehaviorAnimationClipCompletion,
    },
    locomotion::locomotion_types::{Destination, Docking, Route, Steering},
};

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
