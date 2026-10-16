//! Animal catalogue gender, release, and adoption actions.

use super::UiTrigger;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiAnimalActionRecord {
    pub trigger: UiTrigger,
    pub action: UiAnimalAction,
}
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiAnimalAction {
    SetAdoptionCatalogueGenderFilter { gender: UiAnimalGender },
    PutSelectedAnimalUpForAdoption,
    ReleaseSelectedAnimalToWild,
    DeclineAllCurrentAdoptionOffers,
}
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAnimalGender {
    Female,
    Male,
}
