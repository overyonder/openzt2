use bevy::prelude::*;

use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::feeding::container_quantity::DrinkContainer;
use openzt2_zoo_state::feeding::container_quantity::FoodContainer;

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
};

/// Animals running a task on a food or drink container, and the food left.
pub(super) fn collect_animal_task_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    animals: Query<&BehaviorTaskExecutionState, With<Animal>>,
    food: Query<&FoodContainer>,
    drink: Query<(), With<DrinkContainer>>,
) {
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
    ];
    for (name, value) in facts {
        report.current_facts.insert(name.to_owned(), value);
    }
}
