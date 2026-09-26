use bevy::prelude::*;

use crate::plugins::{
    animal_health::types::Dead, simulation_time::simulation_clock_types::ZooClock,
};

use super::{
    animal_age_resolution::resolve_animal_life_stage_from_elapsed_age_ticks,
    types::{Age, Animal, AnimalLifeStage, AuthoredAnimalLifeStageStartTicks},
};

pub(super) fn advance_animal_life_stages_from_elapsed_simulation_time(
    zoo_clock: Res<ZooClock>,
    mut animals: Query<
        (
            &AuthoredAnimalLifeStageStartTicks,
            &Age,
            &mut AnimalLifeStage,
        ),
        (With<Animal>, Without<Dead>),
    >,
) {
    for (life_history, age, mut current_life_stage) in &mut animals {
        // A zero adult threshold provides no maturation schedule. Keep the
        // stage selected by the animal's definition until it morphs.
        if life_history.stage_start_ticks[2] == 0 {
            continue;
        }
        let resolved_life_stage = resolve_animal_life_stage_from_elapsed_age_ticks(
            age.ticks_at(zoo_clock.tick),
            life_history.stage_start_ticks,
        );
        if current_life_stage.0 != resolved_life_stage {
            current_life_stage.0 = resolved_life_stage;
        }
    }
}
