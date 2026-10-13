//! Animal adoption selection, placement, payment, and completion contracts.

use bevy::prelude::*;
use openzt2_game_data::{species::Sex, AssetId};

use crate::economy::money_types::Money;

/// Adoption catalogue gender filter owned by one projected adoption document.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimalCatalogueGender(pub Sex);

/// Requests placement of a selected species through the animal-adoption flow.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeginAnimalAdoptionPlacement {
    pub species: AssetId,
    pub sex: Option<Sex>,
    pub offer_slot_index: Option<u16>,
}

/// Animal-specific state retained on the ordinary construction cursor while
/// the player selects the habitat and world position for an adoption.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimalSpeciesAwaitingHabitatPlacement {
    pub species: AssetId,
    pub variant: Option<AssetId>,
    pub sex: Option<Sex>,
    pub offer_slot_index: Option<u16>,
}

#[derive(Message, Debug, Clone)]
pub struct AdoptAnimal {
    pub species: AssetId,
    pub variant: Option<AssetId>,
    pub sex: Option<Sex>,
    pub offer_slot_index: Option<u16>,
    pub habitat: Entity,
    pub transform: Transform,
}

#[derive(Component, Debug, Clone)]
pub struct PendingAdoption {
    pub species: AssetId,
    pub variant: AssetId,
    pub sex: Sex,
    pub offer_slot_index: Option<u16>,
    pub habitat: Entity,
    pub transform: Transform,
    pub cost: Money,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimalAdopted {
    pub operation: Entity,
    pub animal: Entity,
    pub cost: Money,
    pub species: AssetId,
    pub sex: Sex,
    pub offer_slot_index: Option<u16>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeclineCurrentAnimalAdoptionOffers;
