mod calculations;
mod systems;
pub(crate) mod types;

use types::*;

use bevy::prelude::*;

use crate::{
    application_lifecycle::GamePhase, application_schedule::FixedGameSet,
    plugins::simulation_time::simulation_control_application::simulation_is_running,
};

pub(crate) struct AnimalWelfarePlugin;

impl Plugin for AnimalWelfarePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AdjustHunger>()
            .add_message::<AdjustThirst>()
            .add_message::<AdjustRest>()
            .add_message::<AdjustPrivacy>()
            .add_message::<AdjustSocial>()
            .add_message::<AdjustExercise>()
            .add_message::<AdjustStimulation>()
            .add_message::<AdjustEnvironment>()
            .add_message::<AdjustHealthNeed>()
            .add_message::<AdjustHygiene>()
            .add_message::<AdjustBathroom>()
            .add_message::<SetAnimalNeed>()
            .add_message::<NeedChanged>()
            .add_message::<WelfareChanged>()
            .add_message::<AccumulateNeedPoints>()
            .add_systems(
                FixedUpdate,
                (
                    systems::decay_hunger,
                    systems::decay_thirst,
                    systems::decay_rest,
                    systems::decay_privacy,
                    systems::decay_social,
                    systems::decay_exercise,
                    systems::decay_stimulation,
                    systems::decay_environment,
                    systems::decay_health_need,
                    systems::decay_hygiene,
                    systems::decay_bathroom,
                )
                    .in_set(FixedGameSet::Clock)
                    .run_if(in_state(GamePhase::InGame))
                    .run_if(simulation_is_running),
            )
            .add_systems(
                FixedUpdate,
                (
                    systems::adjust_hunger,
                    systems::adjust_thirst,
                    systems::adjust_rest,
                    systems::adjust_privacy,
                    systems::adjust_social,
                    systems::adjust_exercise,
                    systems::adjust_stimulation,
                    systems::adjust_environment,
                    systems::adjust_health_need,
                    systems::adjust_hygiene,
                    systems::adjust_bathroom,
                    systems::set_animal_need,
                    systems::accumulate_need_points,
                )
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame))
                    .run_if(simulation_is_running),
            )
            .add_systems(
                FixedUpdate,
                (
                    systems::assess_habitat_suitability,
                    systems::derive_animal_welfare,
                )
                    .chain()
                    .in_set(FixedGameSet::Think)
                    .run_if(in_state(GamePhase::InGame))
                    .run_if(simulation_is_running),
            )
            .add_systems(
                FixedUpdate,
                systems::initialize_animal_needs
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}

#[cfg(test)]
mod tests;
