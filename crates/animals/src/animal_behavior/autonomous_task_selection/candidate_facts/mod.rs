//! Borrowed animal facts for the supported autonomous task boundary.

use bevy::{ecs::query::QueryData, prelude::*};
use openzt2_game_data::{
    behavior::eligibility::*,
    species::{LifeStage, Sex},
    AssetId,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_health::types::Disease;
use openzt2_zoo_state::animal_health::types::Frozen;
use openzt2_zoo_state::animal_health::types::Rampaging;
use openzt2_zoo_state::animal_lifecycle::types::AnimalLifeStage;
use openzt2_zoo_state::animal_lifecycle::types::AnimalSex;
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::animal_lifecycle::types::{Mate, Pregnancy};
use openzt2_zoo_state::animal_welfare::types::BathroomNeed;
use openzt2_zoo_state::animal_welfare::types::EnvironmentNeed;
use openzt2_zoo_state::animal_welfare::types::ExerciseNeed;
use openzt2_zoo_state::animal_welfare::types::HealthNeed;
use openzt2_zoo_state::animal_welfare::types::Hunger;
use openzt2_zoo_state::animal_welfare::types::HygieneNeed;
use openzt2_zoo_state::animal_welfare::types::NeedTriggerState;
use openzt2_zoo_state::animal_welfare::types::PrivacyNeed;
use openzt2_zoo_state::animal_welfare::types::RestNeed;
use openzt2_zoo_state::animal_welfare::types::SocialNeed;
use openzt2_zoo_state::animal_welfare::types::StimulationNeed;
use openzt2_zoo_state::animal_welfare::types::Thirst;
use openzt2_zoo_state::animal_welfare::types::MAX_NEED_Q16;
use openzt2_zoo_state::animal_welfare::types::Q16_ONE;
use openzt2_zoo_state::animal_welfare::types::{ReproductionNeed, REPRODUCTION_TRIGGER_INDEX};
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::locomotion::locomotion_types::LocomotionMode;
use openzt2_zoo_state::locomotion::locomotion_types::Velocity;
use std::sync::LazyLock;

static LIVE_STATE_TYPES: LazyLock<[(AssetId, BehaviorEntityStateEligibilityFact); 9]> =
    LazyLock::new(|| {
        [
            ("b_adult", BehaviorEntityStateEligibilityFact::Adult),
            ("b_old", BehaviorEntityStateEligibilityFact::Old),
            ("b_male", BehaviorEntityStateEligibilityFact::Male),
            ("b_pregnant", BehaviorEntityStateEligibilityFact::Pregnant),
            ("b_disease", BehaviorEntityStateEligibilityFact::Diseased),
            ("b_rampage", BehaviorEntityStateEligibilityFact::Rampaging),
            ("b_frozen", BehaviorEntityStateEligibilityFact::Frozen),
            ("b_dead", BehaviorEntityStateEligibilityFact::Dead),
            ("b_moving", BehaviorEntityStateEligibilityFact::Moving),
        ]
        .map(|(name, fact)| (AssetId::from_key(name), fact))
    });
static ANIMAL_TYPES: LazyLock<[AssetId; 2]> =
    LazyLock::new(|| ["animal", "b_animal"].map(AssetId::from_key));
static NEED_NAMES: LazyLock<[AssetId; 11]> = LazyLock::new(|| {
    [
        "hunger",
        "thirst",
        "rest",
        "privacy",
        "social",
        "exercise",
        "stimulation",
        "environment",
        "health",
        "hygiene",
        "bathroom",
    ]
    .map(AssetId::from_key)
});

#[derive(QueryData)]
pub(crate) struct AnimalCandidateFacts {
    pub(crate) species: &'static SpeciesHandle,
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
    water_contact:
        Option<&'static openzt2_zoo_state::locomotion::locomotion_types::AnimalTerrainWaterContact>,
    triggers: &'static NeedTriggerState,
    pregnant: Has<Pregnancy>,
    diseased: Has<Disease>,
    rampaging: Has<Rampaging>,
    frozen: Has<Frozen>,
    velocity: Option<&'static Velocity>,
    habitat: Option<&'static HabitatMember>,
    mate: Has<Mate>,
    reproduction: Option<&'static ReproductionNeed>,
}

impl AnimalCandidateFactsItem<'_, '_> {
    pub(crate) fn variant_id(&self) -> AssetId {
        self.variant.0
    }

    pub(super) fn is_adult(&self) -> bool {
        self.life_stage.0 == LifeStage::Adult
    }

    pub(super) fn habitat_entity(&self) -> Option<Entity> {
        self.habitat.map(|member| member.habitat_entity)
    }

    pub(crate) fn matches_type(&self, required: AssetId, authored_types: &[AssetId]) -> bool {
        // Live state takes precedence over the variant's authored initial flags.
        for &(identifier, fact) in LIVE_STATE_TYPES.iter() {
            if required == identifier {
                return self.boolean_state(fact) == Some(true);
            }
        }
        authored_types.contains(&required)
            || [self.species.species, self.variant.0].contains(&required)
            || ANIMAL_TYPES.contains(&required)
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
            BehaviorEntityStateEligibilityFact::InHabitat => self.habitat.is_some(),
            BehaviorEntityStateEligibilityFact::HasMate => self.mate,
            _ => return None,
        })
    }

    /// Canonical wellness uses 0..1000; authored BFAI values are deprivation 0..100.
    pub(super) fn source_need(&self, name: AssetId) -> Option<(i32, bool)> {
        let needs = [
            self.hunger.0,
            self.thirst.0,
            self.rest.0,
            self.privacy.0,
            self.social.0,
            self.exercise.0,
            self.stimulation.0,
            self.environment.0,
            self.health.0,
            self.hygiene.0,
            self.bathroom.0,
        ];
        let deprivation = |wellness: i32| (MAX_NEED_Q16 - wellness.clamp(0, MAX_NEED_Q16)) / 10;
        if name == AssetId::from_key("reproduction") {
            return self.reproduction.map(|need| {
                (
                    deprivation(need.0),
                    self.triggers.0 & (1 << REPRODUCTION_TRIGGER_INDEX) != 0,
                )
            });
        }
        NEED_NAMES.iter().enumerate().find_map(|(index, key)| {
            (*key == name).then_some((
                deprivation(needs[index]),
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
            BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::InWater) => {
                let Some(contact) = self.water_contact else {
                    return false;
                };
                if contact.depth_m.is_some() {
                    Q16_ONE
                } else {
                    0
                }
            }
            BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::OnLand) => {
                let Some(contact) = self.water_contact else {
                    return false;
                };
                if contact.depth_m.is_none() {
                    Q16_ONE
                } else {
                    0
                }
            }
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
                let wellness = match need {
                    BehaviorWelfareEligibilityFact::Hunger => self.hunger.0,
                    BehaviorWelfareEligibilityFact::Bathroom => self.bathroom.0,
                    BehaviorWelfareEligibilityFact::Thirst => self.thirst.0,
                    BehaviorWelfareEligibilityFact::Rest => self.rest.0,
                    BehaviorWelfareEligibilityFact::Privacy => self.privacy.0,
                    BehaviorWelfareEligibilityFact::Social => self.social.0,
                    BehaviorWelfareEligibilityFact::Exercise => self.exercise.0,
                    BehaviorWelfareEligibilityFact::Stimulation => self.stimulation.0,
                    BehaviorWelfareEligibilityFact::Health => self.health.0,
                    BehaviorWelfareEligibilityFact::Hygiene => self.hygiene.0,
                    BehaviorWelfareEligibilityFact::Reproduction => {
                        let Some(need) = self.reproduction else {
                            return false;
                        };
                        need.0
                    }
                    _ => return false,
                };
                (MAX_NEED_Q16 - wellness.clamp(0, MAX_NEED_Q16)) / 10
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
