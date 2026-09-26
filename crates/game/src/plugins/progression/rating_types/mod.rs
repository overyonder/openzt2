use bevy::prelude::*;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ZooRating {
    pub animal_welfare_permille: u16,
    pub guest_satisfaction_permille: u16,
    pub education_permille: u16,
    pub variety_permille: u16,
    pub scenery_permille: u16,
    pub finance_permille: u16,
    pub cleanliness_permille: u16,
    pub overall_permille: u16,
    // Availability is rebuilt from loaded definitions and live inputs after loading a save.
    pub(crate) cleanliness_available: bool,
    pub(crate) overall_available: bool,
}
