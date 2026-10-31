use bevy::prelude::*;

use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_welfare::types::{AdjustHunger, Hunger, NeedTriggerState};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::feeding::container_quantity::DrinkContainer;
use openzt2_zoo_state::feeding::container_quantity::FoodContainer;

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
};

/// Animals running a task on a food or drink container, the food left, and
/// animal hunger. Hunger improvements, such as eating's authored `hunger`
/// completion, are counted every frame because they fall between snapshots.
pub(super) fn collect_animal_task_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    animals: Query<&BehaviorTaskExecutionState, With<Animal>>,
    hunger: Query<&Hunger, With<Animal>>,
    triggers: Query<&NeedTriggerState, With<Animal>>,
    mut hunger_changes: MessageReader<AdjustHunger>,
    mut hunger_improvements: Local<u32>,
    food: Query<&FoodContainer>,
    drink: Query<(), With<DrinkContainer>>,
) {
    *hunger_improvements += hunger_changes
        .read()
        .filter(|change| change.delta_q16 > 0)
        .count() as u32;
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    let targeting = |matches: &dyn Fn(Entity) -> bool| {
        animals
            .iter()
            .filter(|task| task.target.is_some_and(|target| matches(target)))
            .count()
    };
    let count = |count: usize| f64::from(u32::try_from(count).unwrap_or(u32::MAX));
    let food_left_q16 = food
        .iter()
        .map(|container| i64::from(container.amount_q16))
        .sum::<i64>();
    #[allow(clippy::cast_precision_loss)]
    let facts = [
        (
            "animals_targeting_food",
            count(targeting(&|target| food.contains(target))),
        ),
        (
            "animals_targeting_drink",
            count(targeting(&|target| drink.contains(target))),
        ),
        ("food_left", food_left_q16 as f64 / 65_536.0),
        (
            "animal_hunger_improvements",
            f64::from(*hunger_improvements),
        ),
        // Hunger is need index 0; its bit sets at the species' trigger threshold.
        (
            "animals_hungry",
            count(triggers.iter().filter(|state| state.0 & 1 != 0).count()),
        ),
        (
            "animal_hunger_mean",
            hunger
                .iter()
                .map(|hunger| f64::from(hunger.0) / 65_536.0)
                .sum::<f64>()
                / hunger.iter().len().max(1) as f64,
        ),
    ];
    for (name, value) in facts {
        report.current_facts.insert(name.to_owned(), value);
    }
}
