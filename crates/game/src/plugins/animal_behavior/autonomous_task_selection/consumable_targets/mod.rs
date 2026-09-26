//! Reads the existing consumable owner for concrete, placed target candidates.

use super::candidate_facts::compare_q16;
use crate::plugins::{
    feeding::container_quantity::{DrinkContainer, FoodContainer},
    world_spawn::world_membership_types::{DefinitionId, WorldMember},
};
use bevy::{ecs::query::QueryData, prelude::*};
use openzt2_game_data::{
    behavior::eligibility::{
        BehaviorCandidateEligibilityFact, BehaviorConditionEligibilityFact,
        BehaviorEligibilityFactInput, BehaviorEligibilityFactValue,
        BehaviorEntityStateEligibilityFact, BehaviorSpatialEligibilityFact,
    },
    AssetId,
};

#[derive(QueryData)]
pub(in crate::plugins::animal_behavior) struct ConsumableCandidateFacts {
    pub(super) entity: Entity,
    pub(super) definition: &'static DefinitionId,
    pub(super) transform: &'static GlobalTransform,
    pub(super) world: &'static WorldMember,
    food: Option<&'static FoodContainer>,
    drink: Option<&'static DrinkContainer>,
}

impl ConsumableCandidateFactsItem<'_, '_> {
    pub(super) fn matches_type(&self, required: AssetId) -> bool {
        // Exact target definitions are sufficient for FoodDish_Water and
        // TermiteMound_Insects; inherited target taxonomy is not inferred.
        self.definition.0 == required
    }

    pub(super) fn source_quantity(&self) -> Option<i32> {
        self.food
            .filter(|food| food.food == self.definition.0)
            .map(|food| food.amount_q16)
            .or_else(|| {
                self.drink
                    .filter(|drink| drink.drink == self.definition.0)
                    .map(|drink| drink.amount_q16)
            })
    }

    pub(super) fn satisfies(
        &self,
        fact: &BehaviorCandidateEligibilityFact,
        habitat_membership: Option<bool>,
    ) -> bool {
        let BehaviorEligibilityFactValue::Q16(expected) = fact.expected_value else {
            return false;
        };
        let value = match fact.fact_input {
            BehaviorEligibilityFactInput::Condition(
                BehaviorConditionEligibilityFact::FoodLevel,
            ) => {
                let Some(quantity) = self.source_quantity() else {
                    return false;
                };
                quantity
            }
            BehaviorEligibilityFactInput::Spatial(
                BehaviorSpatialEligibilityFact::SupportedSurface,
            ) => 1 << 16,
            BehaviorEligibilityFactInput::State(BehaviorEntityStateEligibilityFact::InHabitat) => {
                let Some(in_habitat) = habitat_membership else {
                    return false;
                };
                i32::from(in_habitat) << 16
            }
            _ => return false,
        };
        compare_q16(value, expected, fact.comparison)
    }
}

pub(super) fn can_evaluate_consumable_fact(
    fact: &BehaviorCandidateEligibilityFact,
    habitat_membership: Option<bool>,
) -> bool {
    if matches!(
        fact.fact_input,
        BehaviorEligibilityFactInput::State(BehaviorEntityStateEligibilityFact::InHabitat)
    ) {
        return habitat_membership.is_some()
            && matches!(fact.expected_value, BehaviorEligibilityFactValue::Q16(_));
    }
    matches!(fact.expected_value, BehaviorEligibilityFactValue::Q16(_))
        && matches!(
            fact.fact_input,
            BehaviorEligibilityFactInput::Condition(BehaviorConditionEligibilityFact::FoodLevel)
                | BehaviorEligibilityFactInput::Spatial(
                    BehaviorSpatialEligibilityFact::SupportedSurface
                )
        )
}
