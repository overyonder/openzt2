use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum BehaviorScore {
    Fixed(f32),
    DistanceInfluence(bool),
    LeaveZooPressure,
    AggregateGoodNeeds {
        weight: f32,
    },
    AggregateBadNeeds {
        weight: f32,
    },
    /// Numeric entry of the evaluation's `BFAIAttributeFloatMap`, not a qualifier.
    AttributeValue {
        attribute: crate::AssetId,
        value: super::scalar::BehaviorScalarQ16,
    },
    /// Authored biome weight, retained separately from need adjustments.
    BiomeWeight {
        biome: crate::AssetId,
        weight: f32,
    },
}
