//! Authored zoo-rating inputs, fame thresholds, and progression awards.

use crate::AssetId;

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum RatingInputKind {
    AnimalWelfare,
    GuestSatisfaction,
    Education,
    Variety,
    Scenery,
    Finance,
    Cleanliness,
}

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct RatingDefinition {
    pub id: AssetId,
    pub inputs: Vec<RatingInput>,
    pub minimum: u16,
    pub maximum: u16,
    pub smoothing_ticks: u32,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct RatingInput {
    pub kind: RatingInputKind,
    pub weight: i16,
    pub minimum: i32,
    pub maximum: i32,
}

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct FameThreshold {
    pub level: u16,
    pub minimum_rating: u16,
    pub minimum_guests: u32,
    pub unlocks: Vec<AssetId>,
}

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct AwardDefinition {
    pub id: AssetId,
    pub name_key: AssetId,
    pub description_key: AssetId,
    pub conditions: Vec<AwardCondition>,
    pub reward_cents: i64,
    pub unlocks: Vec<AssetId>,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub enum Comparison {
    AtLeast,
    AtMost,
    Equal,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct AwardCondition {
    pub kind: RatingInputKind,
    pub comparison: Comparison,
    pub value: i32,
    pub duration_ticks: u64,
}
