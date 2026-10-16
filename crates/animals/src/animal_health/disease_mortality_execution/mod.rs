use bevy::prelude::*;

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::Docking;
use openzt2_zoo_state::locomotion::locomotion_types::Route;
use openzt2_zoo_state::locomotion::locomotion_types::Steering;
use openzt2_zoo_state::locomotion::locomotion_types::{
    ContactSteering, DirectLocomotion, MotionProgress, NavAgent, SpatialCell, Velocity,
};

use super::types::{
    AnimalDied, Dead, DeathCause, Disease, Escaped, Frozen, Rampaging, RecoveringFromTranquilizer,
    Tranquilized, Treatment, Vitality,
};

type AnimalActivityStateRemovedAfterDeath = (
    Frozen,
    Treatment,
    Tranquilized,
    RecoveringFromTranquilizer,
    Escaped,
    Rampaging,
    BehaviorTaskExecutionState,
    PendingBehaviorAnimationClipCompletion,
    Destination,
    Route,
    Steering,
    Docking,
);

pub(super) fn finalize_disease_deaths_after_vitality_reaches_zero(
    mut commands: Commands,
    clock: Res<ZooClock>,
    live_animals: Query<(Entity, &Vitality, Option<&Disease>), (With<Animal>, Without<Dead>)>,
    mut animal_died_messages: MessageWriter<AnimalDied>,
) {
    for (animal, vitality, disease) in &live_animals {
        if vitality.0 > 0.0 {
            continue;
        }
        let Some(disease) = disease else {
            continue;
        };
        let death_cause = DeathCause::Disease(disease.definition);
        finalize_animal_death(
            &mut commands,
            &mut animal_died_messages,
            animal,
            death_cause,
            clock.tick,
        );
    }
}

pub(crate) fn finalize_animal_death(
    commands: &mut Commands,
    messages: &mut MessageWriter<AnimalDied>,
    animal: Entity,
    cause: DeathCause,
    tick: u64,
) {
    commands
        .entity(animal)
        .insert(Dead {
            cause,
            since_tick: tick,
        })
        .remove::<AnimalActivityStateRemovedAfterDeath>();
    commands.entity(animal).remove::<(
        NavAgent,
        DirectLocomotion,
        ContactSteering,
        SpatialCell,
        MotionProgress,
        Velocity,
    )>();
    messages.write(AnimalDied { animal, cause });
}
