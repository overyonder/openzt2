use bevy::prelude::*;

use crate::plugins::{
    animal_lifecycle::types::Animal,
    behavior_task_execution_types::{
        BehaviorTaskExecutionState, PendingBehaviorAnimationClipCompletion,
    },
    locomotion::locomotion_types::{Destination, Docking, Route, Steering},
    simulation_time::simulation_clock_types::ZooClock,
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
        commands.entity(animal).insert(Dead {
            cause: death_cause,
            since_tick: clock.tick,
        });
        commands
            .entity(animal)
            .remove::<AnimalActivityStateRemovedAfterDeath>();
        animal_died_messages.write(AnimalDied {
            animal,
            cause: death_cause,
        });
    }
}
