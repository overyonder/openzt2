use bevy::prelude::*;

use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::Steering;

use super::types::{Dead, FreezeAnimal, Frozen, ThawAnimal};

pub(super) fn stop_animal_activity(animal: Entity, commands: &mut Commands) {
    openzt2_zoo_state::behavior_task_execution_types::abandon_behavior_task_for_reconsideration(
        animal, commands,
    );
    commands.entity(animal).try_remove::<(Route, Steering)>();
}

pub(super) fn stop_activity_when_frozen(
    frozen: On<Add<Frozen>>,
    animals: Query<(), With<Animal>>,
    mut commands: Commands,
) {
    if animals.contains(frozen.entity) {
        stop_animal_activity(frozen.entity, &mut commands);
    }
}

pub(super) fn cancel_tasks_started_for_inactive_animals(
    started: On<Insert<BehaviorTaskExecutionState>>,
    inactive: Query<(), (With<Animal>, Or<(With<Frozen>, With<Dead>)>)>,
    mut commands: Commands,
) {
    if inactive.contains(started.entity) {
        stop_animal_activity(started.entity, &mut commands);
    }
}

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
        commands.entity(request.animal).try_insert(Frozen);
    }
    for request in thaw_animal_requests.read() {
        if live_animals.get(request.animal).is_ok() {
            commands.entity(request.animal).remove::<Frozen>();
        }
    }
}
