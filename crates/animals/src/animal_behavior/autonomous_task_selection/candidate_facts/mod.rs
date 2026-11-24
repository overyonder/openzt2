//! Borrowed animal facts for the supported autonomous task boundary.

use crate::animal_welfare::need_latch_types::AnimalNeedLatches;
use bevy::{ecs::query::QueryData, prelude::*};
use openzt2_game_data::{
    AssetId,
    behavior::eligibility::*,
    species::{LifeStage, Sex},
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::animal_lineage::AnimalMate;
use openzt2_zoo_state::animal_health::types::Disease;
use openzt2_zoo_state::animal_health::types::Frozen;
use openzt2_zoo_state::animal_health::types::Rampaging;
use openzt2_zoo_state::animal_lifecycle::types::Age;
use openzt2_zoo_state::animal_lifecycle::types::AnimalLifeStage;
use openzt2_zoo_state::animal_lifecycle::types::AnimalSex;
use openzt2_zoo_state::animal_lifecycle::types::AnimalVariant;
use openzt2_zoo_state::animal_lifecycle::types::Pregnancy;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::animal_welfare::types::BathroomNeed;
use openzt2_zoo_state::animal_welfare::types::EnvironmentNeed;
use openzt2_zoo_state::animal_welfare::types::ExerciseNeed;
use openzt2_zoo_state::animal_welfare::types::HealthNeed;
use openzt2_zoo_state::animal_welfare::types::Hunger;
use openzt2_zoo_state::animal_welfare::types::HygieneNeed;
use openzt2_zoo_state::animal_welfare::types::MAX_NEED_Q16;
use openzt2_zoo_state::animal_welfare::types::PrivacyNeed;
use openzt2_zoo_state::animal_welfare::types::Q16_ONE;
use openzt2_zoo_state::animal_welfare::types::RestNeed;
use openzt2_zoo_state::animal_welfare::types::SocialNeed;
use openzt2_zoo_state::animal_welfare::types::StimulationNeed;
use openzt2_zoo_state::animal_welfare::types::Thirst;
use openzt2_zoo_state::animal_welfare::types::{REPRODUCTION_TRIGGER_INDEX, ReproductionNeed};
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

pub(super) fn animal_subject_type_can_match(
    required: AssetId,
    authored_types: &[AssetId],
    species: AssetId,
    variant: AssetId,
) -> bool {
    is_live_animal_state_type(required)
        || matches_static_type(required, authored_types, species, variant)
}

pub(super) fn is_live_animal_state_type(required: AssetId) -> bool {
    LIVE_STATE_TYPES.iter().any(|&(id, _)| id == required)
}

fn matches_static_type(
    required: AssetId,
    authored_types: &[AssetId],
    species: AssetId,
    variant: AssetId,
) -> bool {
    authored_types.contains(&required)
        || [species, variant].contains(&required)
        || ANIMAL_TYPES.contains(&required)
}
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
    age: Option<&'static Age>,
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
    pub(crate) transform: &'static GlobalTransform,
    pub(crate) world: &'static WorldMember,
    mode: Option<&'static LocomotionMode>,
    swim_layer: Option<&'static openzt2_zoo_state::locomotion::locomotion_types::SwimLayer>,
    water_contact:
        Option<&'static openzt2_zoo_state::locomotion::locomotion_types::AnimalTerrainWaterContact>,
    need_latches: AnimalNeedLatches,
    pregnant: Has<Pregnancy>,
    diseased: Has<Disease>,
    rampaging: Has<Rampaging>,
    frozen: Has<Frozen>,
    dead: Has<openzt2_zoo_state::animal_health::types::Dead>,
    velocity: Option<&'static Velocity>,
    habitat: Option<&'static HabitatMember>,
    pub(super) mate: Option<&'static AnimalMate>,
    reproduction: Option<&'static ReproductionNeed>,
    pub(super) escaped: Has<openzt2_zoo_state::animal_health::types::Escaped>,
    need_points: Option<&'static openzt2_zoo_state::animal_welfare::types::NeedPoints>,
    training: Option<&'static openzt2_zoo_state::shows::performer_training::PerformerTraining>,
    rehearsing: Has<openzt2_zoo_state::shows::running_show_types::AnimalPerformingShowTrick>,
    authored_values: Option<&'static crate::animal_behavior::ScriptFloatAttributes>,
}

impl AnimalCandidateFactsItem<'_, '_> {
    /// The current good and bad need points.
    pub(super) fn need_points(&self) -> (f32, f32) {
        self.need_points
            .map_or((0.0, 0.0), |points| (points.good, points.bad))
    }

    pub(crate) fn variant_id(&self) -> AssetId {
        self.variant.0
    }

    pub(in crate::animal_behavior) fn is_adult(&self) -> bool {
        self.life_stage.0 == LifeStage::Adult
    }

    pub(super) fn is_dead(&self) -> bool {
        self.dead
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
        matches_static_type(
            required,
            authored_types,
            self.species.species,
            self.variant.0,
        )
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
            BehaviorEntityStateEligibilityFact::Dead => self.dead,
            BehaviorEntityStateEligibilityFact::Moving => self.velocity?.0.length_squared() > 0.0,
            BehaviorEntityStateEligibilityFact::Swimming => *self.mode? == LocomotionMode::Swim,
            BehaviorEntityStateEligibilityFact::InHabitat => self.habitat.is_some(),
            BehaviorEntityStateEligibilityFact::HasMate => self.mate.is_some(),
            BehaviorEntityStateEligibilityFact::InShow => self.rehearsing,
            _ => return None,
        })
    }

    pub(super) fn age_q16(&self, tick: u64, ticks_per_day: u32) -> Option<i32> {
        let age = self.age.filter(|_| ticks_per_day != 0)?;
        Some(
            (age.ticks_at(tick) as f64 * f64::from(Q16_ONE) / f64::from(ticks_per_day)).round()
                as i32,
        )
    }

    /// Canonical wellness uses 0..1000; authored BFAI values are deprivation 0..100.
    pub(super) fn source_need(&self, name: AssetId) -> Option<(i32, bool)> {
        if name == AssetId::from_key("breath") {
            let value = self.authored_value(name);
            return Some((value, value > 0));
        }
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
                    self.need_latches.triggered(REPRODUCTION_TRIGGER_INDEX),
                )
            });
        }
        NEED_NAMES.iter().enumerate().find_map(|(index, key)| {
            (*key == name).then(|| {
                (
                    deprivation(needs[index]),
                    self.need_latches.triggered(index),
                )
            })
        })
    }

    pub(super) fn satisfies(
        &self,
        fact: &BehaviorCandidateEligibilityFact,
        tick: u64,
        ticks_per_day: u32,
        authored_types: &[AssetId],
    ) -> bool {
        use openzt2_zoo_state::locomotion::locomotion_types::SwimLayer;
        if fact.fact_input
            == BehaviorEligibilityFactInput::Relationship(
                BehaviorRelationshipEligibilityFact::HasTrick,
            )
        {
            return match fact.expected_value {
                BehaviorEligibilityFactValue::SymbolThreshold { symbol, q16 } => {
                    let score = self
                        .training
                        .map_or(0, |training| training.proficiency(symbol));
                    i64::from(score) * i64::from(Q16_ONE) >= i64::from(q16)
                }
                BehaviorEligibilityFactValue::Symbol(trick) => self
                    .training
                    .is_some_and(|training| training.proficiency(trick) != 0),
                BehaviorEligibilityFactValue::Q16(_) => false,
            };
        }
        let BehaviorEligibilityFactValue::Q16(expected) = fact.expected_value else {
            return false;
        };
        let actual = match fact.fact_input {
            BehaviorEligibilityFactInput::Spatial(
                BehaviorSpatialEligibilityFact::SupportedSurface,
            ) => Q16_ONE,
            BehaviorEligibilityFactInput::Spatial(
                BehaviorSpatialEligibilityFact::SwimmingSurface,
            ) => {
                i32::from(
                    self.mode == Some(&LocomotionMode::Swim)
                        && self.swim_layer == Some(&SwimLayer::Surface),
                ) * Q16_ONE
            }
            BehaviorEligibilityFactInput::Spatial(
                BehaviorSpatialEligibilityFact::SwimmingUnderwater,
            ) => {
                i32::from(
                    self.mode == Some(&LocomotionMode::Swim)
                        && self.swim_layer == Some(&SwimLayer::Submerged),
                ) * Q16_ONE
            }
            BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::WaterDepth) => {
                let Some(depth) = self.water_contact.and_then(|contact| contact.depth_m) else {
                    return false;
                };
                (depth * Q16_ONE as f32).round() as i32
            }
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
                let value = self.boolean_state(state).unwrap_or_else(|| {
                    let key = match state {
                        BehaviorEntityStateEligibilityFact::Attacked => "b_attacked",
                        BehaviorEntityStateEligibilityFact::Electric => "b_electric",
                        BehaviorEntityStateEligibilityFact::InCloningCenter => "b_incloningcenter",
                        BehaviorEntityStateEligibilityFact::NurseYoung => "b_nurseyoung",
                        BehaviorEntityStateEligibilityFact::ScentEmitter => "b_scentemitter",
                        BehaviorEntityStateEligibilityFact::Teleportable => "b_teleport",
                        BehaviorEntityStateEligibilityFact::Talking => "b_talking",
                        BehaviorEntityStateEligibilityFact::InTrainingArea => "issubjectinta",
                        BehaviorEntityStateEligibilityFact::Floating => "isfloating",
                        BehaviorEntityStateEligibilityFact::Elevated => "iselevated",
                        _ => return false,
                    };
                    authored_types.contains(&AssetId::from_key(key))
                });
                if value { Q16_ONE } else { 0 }
            }
            BehaviorEligibilityFactInput::Taxonomy(taxonomy) => {
                let key = match taxonomy {
                    BehaviorTaxonomyEligibilityFact::Felidae => "b_felidae",
                    BehaviorTaxonomyEligibilityFact::MarineAnimal => "b_marineanimal",
                    BehaviorTaxonomyEligibilityFact::SmallPredator => "b_smallpredator",
                    BehaviorTaxonomyEligibilityFact::MediumPredator => "b_mediumpredator",
                    BehaviorTaxonomyEligibilityFact::LargePredator => "b_largepredator",
                    BehaviorTaxonomyEligibilityFact::ExtraLargePredator => "b_xlargepredator",
                    BehaviorTaxonomyEligibilityFact::MediumPrey => "b_mediumprey",
                    BehaviorTaxonomyEligibilityFact::Biome => return false,
                };
                i32::from(authored_types.contains(&AssetId::from_key(key))) * Q16_ONE
            }
            BehaviorEligibilityFactInput::Condition(
                BehaviorConditionEligibilityFact::NeedPointsGood,
            ) => (self.need_points().0 * Q16_ONE as f32).round() as i32,
            BehaviorEligibilityFactInput::Condition(
                BehaviorConditionEligibilityFact::NeedPointsBad,
            ) => (self.need_points().1 * Q16_ONE as f32).round() as i32,
            BehaviorEligibilityFactInput::Welfare(BehaviorWelfareEligibilityFact::Lifespan) => {
                let Some(age) = self.age_q16(tick, ticks_per_day) else {
                    return false;
                };
                age
            }
            BehaviorEligibilityFactInput::Welfare(BehaviorWelfareEligibilityFact::Breath) => {
                self.authored_value(AssetId::from_key("breath"))
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
                    BehaviorWelfareEligibilityFact::Space => self.environment.0,
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

    fn authored_value(&self, key: AssetId) -> i32 {
        (self
            .authored_values
            .and_then(|values| values.0.get(&key))
            .copied()
            .unwrap_or_default()
            * Q16_ONE as f32)
            .round() as i32
    }
}

pub(crate) fn compare_q16(
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
