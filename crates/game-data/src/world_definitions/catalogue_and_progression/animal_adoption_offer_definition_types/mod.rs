//! Authored animal-adoption offer inventory configuration.

use crate::AssetId;

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct AnimalAdoptionOfferConfiguration {
    pub base_slot_count: u16,
    pub installed_expansion_slot_count: u16,
    pub fame_slots: Vec<AnimalAdoptionFameSlotThreshold>,
}

#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct AnimalAdoptionFameSlotThreshold {
    pub fame_percent: u16,
    pub additional_slot_count: u16,
    pub locked_slot_long_tooltip_key: AssetId,
}
