//! Borrowed animal facts for the supported autonomous task boundary.

use crate::plugins::{
    animal_health::types::{Disease, Frozen, Rampaging},
    animal_lifecycle::types::{
        AnimalLifeStage, AnimalSex, AnimalVariant, Pregnancy, SpeciesHandle,
    },
    animal_welfare::types::{
        BathroomNeed, EnvironmentNeed, ExerciseNeed, HealthNeed, Hunger, HygieneNeed,
        NeedTriggerState, PrivacyNeed, RestNeed, SocialNeed, StimulationNeed, Thirst, MAX_NEED_Q16,
        Q16_ONE,
    },
    locomotion::locomotion_types::{LocomotionMode, Velocity},
    world_spawn::world_membership_types::WorldMember,
};
use bevy::{ecs::query::QueryData, prelude::*};
use openzt2_game_data::{
    behavior::eligibility::*,
    species::{LifeStage, Sex},
    AssetId,
};

#[derive(QueryData)]
pub(in crate::plugins::animal_behavior) struct AnimalCandidateFacts {
    species: &'static SpeciesHandle,
    variant: &'static AnimalVariant,
    life_stage: &'static AnimalLifeStage,
    sex: &'static AnimalSex,
    hunger: &'static Hunger,
    thirst: &'static Thirst,
    rest: &'static RestNeed,
    privacy: &'static PrivacyNeed,
    social: &'static SocialNeed,
    exercise: &'static ExerciseNeed,
    stimulation: &'static StimulationNeed,
    environment: &'static EnvironmentNeed,
    health: &'static HealthNeed,
    hygiene: &'static HygieneNeed,
    bathroom: &'static BathroomNeed,
    pub(super) transform: &'static GlobalTransform,
    pub(super) world: &'static WorldMember,
    mode: Option<&'static LocomotionMode>,
    triggers: &'static NeedTriggerState,
    pregnant: Has<Pregnancy>,
    diseased: Has<Disease>,
    rampaging: Has<Rampaging>,
    frozen: Has<Frozen>,
    velocity: Option<&'static Velocity>,
}

impl AnimalCandidateFactsItem<'_, '_> {
    pub(super) fn variant_id(&self) -> AssetId {
        self.variant.0
    }

    pub(super) fn matches_type(&self, required: AssetId, authored_types: &[AssetId]) -> bool {
        // Live state takes precedence over the variant's authored initial flags.
        for (name, fact) in [
            ("b_adult", BehaviorEntityStateEligibilityFact::Adult),
            ("b_old", BehaviorEntityStateEligibilityFact::Old),
            ("b_male", BehaviorEntityStateEligibilityFact::Male),
            ("b_pregnant", BehaviorEntityStateEligibilityFact::Pregnant),
            ("b_disease", BehaviorEntityStateEligibilityFact::Diseased),
            ("b_rampage", BehaviorEntityStateEligibilityFact::Rampaging),
            ("b_frozen", BehaviorEntityStateEligibilityFact::Frozen),
            ("b_dead", BehaviorEntityStateEligibilityFact::Dead),
            ("b_moving", BehaviorEntityStateEligibilityFact::Moving),
        ] {
            if required == AssetId::from_key(name) {
                return self.boolean_state(fact) == Some(true);
            }
        }
        authored_types.contains(&required)
            || [
                self.species.species,
                self.variant.0,
                AssetId::from_key("animal"),
                AssetId::from_key("b_animal"),
            ]
            .contains(&required)
    }

    fn boolean_state(&self, state: BehaviorEntityStateEligibilityFact) -> Option<bool> {
        Some(match state {
            BehaviorEntityStateEligibilityFact::Adult => self.life_stage.0 == LifeStage::Adult,
            BehaviorEntityStateEligibilityFact::Old => self.life_stage.0 == LifeStage::Elder,
            BehaviorEntityStateEligibilityFact::Male => self.sex.0 == Sex::Male,
            BehaviorEntityStateEligibilityFact::Pregnant => self.pregnant,
            BehaviorEntityStateEligibilityFact::Diseased => self.diseased,
            BehaviorEntityStateEligibilityFact::Rampaging => self.rampaging,
            BehaviorEntityStateEligibilityFact::Frozen => self.frozen,
            BehaviorEntityStateEligibilityFact::Dead => false, // Caller excludes Dead actors.
            BehaviorEntityStateEligibilityFact::Moving => self.velocity?.0.length_squared() > 0.0,
            BehaviorEntityStateEligibilityFact::Swimming => *self.mode? == LocomotionMode::Swim,
            _ => return None,
        })
    }

    /// Canonical wellness uses 0..1000; authored BFAI values are deprivation 0..100.
    pub(super) fn source_need(&self, name: AssetId) -> Option<(i32, bool)> {
        let needs = [
            ("hunger", self.hunger.0),
            ("thirst", self.thirst.0),
            ("rest", self.rest.0),
            ("privacy", self.privacy.0),
            ("social", self.social.0),
            ("exercise", self.exercise.0),
            ("stimulation", self.stimulation.0),
            ("environment", self.environment.0),
            ("health", self.health.0),
            ("hygiene", self.hygiene.0),
            ("bathroom", self.bathroom.0),
        ];
        needs
            .iter()
            .enumerate()
            .find_map(|(index, (key, wellness))| {
                (AssetId::from_key(key) == name).then_some((
                    (MAX_NEED_Q16 - (*wellness).clamp(0, MAX_NEED_Q16)) / 10,
                    self.triggers.0 & (1 << index) != 0,
                ))
            })
    }

    pub(super) fn satisfies(&self, fact: &BehaviorCandidateEligibilityFact) -> bool {
        let BehaviorEligibilityFactValue::Q16(expected) = fact.expected_value else {
            return false;
        };
        let actual = match fact.fact_input {
            BehaviorEligibilityFactInput::Spatial(
                BehaviorSpatialEligibilityFact::SupportedSurface,
            ) => Q16_ONE,
            BehaviorEligibilityFactInput::State(state) => {
                let Some(value) = self.boolean_state(state) else {
                    return false;
                };
                if value {
                    Q16_ONE
                } else {
                    0
                }
            }
            BehaviorEligibilityFactInput::Welfare(need) => {
                let name = match need {
                    BehaviorWelfareEligibilityFact::Hunger => "hunger",
                    BehaviorWelfareEligibilityFact::Bathroom => "bathroom",
                    BehaviorWelfareEligibilityFact::Thirst => "thirst",
                    BehaviorWelfareEligibilityFact::Rest => "rest",
                    BehaviorWelfareEligibilityFact::Privacy => "privacy",
                    BehaviorWelfareEligibilityFact::Social => "social",
                    BehaviorWelfareEligibilityFact::Exercise => "exercise",
                    BehaviorWelfareEligibilityFact::Stimulation => "stimulation",
                    BehaviorWelfareEligibilityFact::Health => "health",
                    BehaviorWelfareEligibilityFact::Hygiene => "hygiene",
                    _ => return false,
                };
                let Some((value, _)) = self.source_need(AssetId::from_key(name)) else {
                    return false;
                };
                value
            }
            _ => return false,
        };
        compare_q16(actual, expected, fact.comparison)
    }
}

pub(super) fn compare_q16(
    actual: i32,
    expected: i32,
    comparison: BehaviorEligibilityFactComparison,
) -> bool {
    match comparison {
        BehaviorEligibilityFactComparison::Equal => actual == expected,
        BehaviorEligibilityFactComparison::NotEqual => actual != expected,
        BehaviorEligibilityFactComparison::Less => actual < expected,
        BehaviorEligibilityFactComparison::LessOrEqual => actual <= expected,
        BehaviorEligibilityFactComparison::Greater => actual > expected,
        BehaviorEligibilityFactComparison::GreaterOrEqual => actual >= expected,
    }
}
