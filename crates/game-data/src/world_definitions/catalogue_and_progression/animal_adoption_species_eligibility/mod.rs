use crate::AssetId;

/// A concrete animal type contributes its species when its positive rarity is
/// at or below the requested fame limit. Selection deduplicates species.
#[allow(
    clippy::derive_partial_eq_without_eq,
    reason = "candidate rarity limits are floating-point values"
)]
#[derive(Clone, Copy, Debug, serde::Deserialize, PartialEq, serde::Serialize)]
pub struct AnimalAdoptionSpeciesEligibility {
    pub id: AssetId,
    pub species: AssetId,
    pub rarity_fame_percent: f32,
}
