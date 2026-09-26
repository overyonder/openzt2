use serde::{Deserialize, Serialize};

use crate::AssetId;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct BehaviorCandidateEligibilityRequirements {
    pub candidate_role: BehaviorCandidateRole,
    pub eligibility_fact_junction: BehaviorEligibilityFactJunction,
    /// Exact authored candidate types, resolved to stable native identities.
    pub candidate_type_identifiers: Vec<AssetId>,
    #[serde(default)]
    pub candidate_type_junction: BehaviorEligibilityFactJunction,
    pub eligibility_facts: Vec<BehaviorCandidateEligibilityFact>,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BehaviorCandidateRole {
    Subject,
    Target,
    Object,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum BehaviorEligibilityFactJunction {
    All,
    #[default]
    Any,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BehaviorCandidateEligibilityFact {
    pub fact_input: BehaviorEligibilityFactInput,
    pub comparison: BehaviorEligibilityFactComparison,
    /// Q16 value for boolean/numeric facts, or a stable `AssetId` for symbolic
    /// taxonomy, relationship, trick, and biome facts.
    pub expected_value: BehaviorEligibilityFactValue,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorEligibilityFactValue {
    Q16(i32),
    Symbol(AssetId),
    SymbolThreshold { symbol: AssetId, q16: i32 },
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BehaviorEligibilityFactComparison {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorEligibilityFactInput {
    State(BehaviorEntityStateEligibilityFact),
    Welfare(BehaviorWelfareEligibilityFact),
    Spatial(BehaviorSpatialEligibilityFact),
    Relationship(BehaviorRelationshipEligibilityFact),
    Taxonomy(BehaviorTaxonomyEligibilityFact),
    Condition(BehaviorConditionEligibilityFact),
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BehaviorEntityStateEligibilityFact {
    Adult,
    Attacked,
    Dead,
    Diseased,
    Electric,
    InCloningCenter,
    Male,
    Moving,
    NurseYoung,
    Old,
    Pregnant,
    Rampaging,
    ScentEmitter,
    Teleportable,
    Talking,
    Frozen,
    CanMate,
    HasMate,
    InHabitat,
    InShow,
    InTrainingArea,
    Swimming,
    Floating,
    Elevated,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BehaviorWelfareEligibilityFact {
    Amusement,
    Bathroom,
    Bloom,
    Breath,
    Dessert,
    Exercise,
    Happiness,
    Health,
    Hunger,
    Hygiene,
    Incubation,
    Lifespan,
    Privacy,
    Reproduction,
    Rest,
    Social,
    Space,
    Stimulation,
    Thirst,
    Unbloom,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BehaviorSpatialEligibilityFact {
    InWater,
    OnLand,
    /// Native `inWater_OR_onLand`: an unconditional true qualifier.
    SupportedSurface,
    SwimmingSurface,
    SwimmingUnderwater,
    DepthAboveBottom,
    DepthBelowSurface,
    ShoreDistance,
    WaterDepth,
    WaterDirtiness,
    InSight,
    NotInSight,
    TimeOfDay,
    TimeOfDayMin,
    TimeOfDayMax,
    InGlacier,
    ElevatedPath,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BehaviorRelationshipEligibilityFact {
    Family,
    Relation,
    SameSpecies,
    HasTrick,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BehaviorTaxonomyEligibilityFact {
    Biome,
    Felidae,
    MarineAnimal,
    SmallPredator,
    MediumPredator,
    LargePredator,
    MediumPrey,
    ExtraLargePredator,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BehaviorConditionEligibilityFact {
    CaffeineTrigger,
    DonationBonus,
    WishFountain,
    NoBuyBackGift,
    NoBuyHeadGift,
    NoBuyLeftHandGift,
    NoBuyRightHandGift,
    NoBuyShirtGift,
    NoBuyWaistGift,
    NoFossilDig,
    ProvidesCover,
    BoneLevel,
    PaintLevel1,
    PaintLevel2,
    PaintLevel3,
    PaintLevel4,
    IceLevel,
    BuildingStrength,
    ConstructionLevel,
    Damage,
    Opened,
    FenceStrength,
    FilterDirtiness,
    FoodLevel,
    TrashLevel,
    DeparturePoints,
    NeedPointsBad,
    NeedPointsGood,
}
