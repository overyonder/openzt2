//! Other animals as task targets, such as the male an aardvark female invites
//! to mate, and the inviting female a `fromToken` task answers.

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::behavior::eligibility::{
    BehaviorCandidateEligibilityFact, BehaviorEligibilityFactInput, BehaviorEligibilityFactValue,
    BehaviorEntityStateEligibilityFact, BehaviorSpatialEligibilityFact,
};
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_lifecycle::types::{Animal, Parents};
use openzt2_zoo_state::animal_welfare::types::Q16_ONE;

use super::candidate_facts::{compare_q16, AnimalCandidateFacts, AnimalCandidateFactsItem};

#[derive(SystemParam)]
pub(crate) struct AnimalTargetSources<'w, 's> {
    pub(super) animals: Query<
        'w,
        's,
        (Entity, AnimalCandidateFacts),
        (With<Animal>, Without<Dead>, Without<WorldEntityIsCrated>),
    >,
    parents: Query<'w, 's, &'static Parents>,
}

impl AnimalTargetSources<'_, '_> {
    /// `canMate`: the target is an adult that isn't the subject's parent, child
    /// or sibling.
    fn can_mate(
        &self,
        subject: Entity,
        target: Entity,
        target_facts: &AnimalCandidateFactsItem,
    ) -> bool {
        let lineage = |animal| {
            self.parents
                .get(animal)
                .ok()
                .copied()
                .unwrap_or(Parents::NONE)
        };
        let (subject_parents, target_parents) = (lineage(subject), lineage(target));
        let parent_of = |parents: Parents, animal| {
            parents.mother == Some(animal) || parents.father == Some(animal)
        };
        let siblings = |a: Option<Entity>, b: Option<Entity>| a.is_some() && a == b;
        target_facts.is_adult()
            && !parent_of(subject_parents, target)
            && !parent_of(target_parents, subject)
            && !siblings(subject_parents.mother, target_parents.mother)
            && !siblings(subject_parents.father, target_parents.father)
    }

    /// Evaluates one authored target fact on another animal, relative to the
    /// subject where the fact is a relationship or a place.
    pub(super) fn target_satisfies(
        &self,
        subject: Entity,
        subject_facts: &AnimalCandidateFactsItem,
        target: Entity,
        target_facts: &AnimalCandidateFactsItem,
        fact: &BehaviorCandidateEligibilityFact,
    ) -> bool {
        let BehaviorEligibilityFactValue::Q16(expected) = fact.expected_value else {
            return false;
        };
        let flag = |value: bool| if value { Q16_ONE } else { 0 };
        let actual = match fact.fact_input {
            BehaviorEligibilityFactInput::State(BehaviorEntityStateEligibilityFact::InHabitat) => {
                flag(
                    subject_facts.habitat_entity().is_some()
                        && subject_facts.habitat_entity() == target_facts.habitat_entity(),
                )
            }
            BehaviorEligibilityFactInput::State(BehaviorEntityStateEligibilityFact::CanMate) => {
                flag(self.can_mate(subject, target, target_facts))
            }
            BehaviorEligibilityFactInput::Spatial(
                kind @ (BehaviorSpatialEligibilityFact::InSight
                | BehaviorSpatialEligibilityFact::NotInSight),
            ) => {
                let inside = subject_facts
                    .transform
                    .translation()
                    .xz()
                    .distance(target_facts.transform.translation().xz())
                    < expected as f32 / 65_536.0;
                return if kind == BehaviorSpatialEligibilityFact::InSight {
                    inside
                } else {
                    !inside
                };
            }
            _ => return target_facts.satisfies(fact),
        };
        compare_q16(actual, expected, fact.comparison)
    }
}
