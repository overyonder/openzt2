//! Animal adoption selection, placement, payment, and completion contracts.

use bevy::prelude::*;
use openzt2_game_data::{species::Sex, AssetId};

use crate::plugins::economy::money_types::Money;

/// Adoption catalogue gender filter owned by one projected adoption document.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnimalCatalogueGender(pub(crate) Sex);

/// Requests placement of a selected species through the animal-adoption flow.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BeginAnimalAdoptionPlacement {
    pub(crate) species: AssetId,
    pub(crate) sex: Option<Sex>,
    pub(crate) offer_slot_index: Option<u16>,
}

/// Animal-specific state retained on the ordinary construction cursor while
/// the player selects the habitat and world position for an adoption.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AnimalSpeciesAwaitingHabitatPlacement {
    pub(super) species: AssetId,
    pub(super) variant: Option<AssetId>,
    pub(super) sex: Option<Sex>,
    pub(super) offer_slot_index: Option<u16>,
}

#[derive(Message, Debug, Clone)]
pub(super) struct AdoptAnimal {
    pub(super) species: AssetId,
    pub(super) variant: Option<AssetId>,
    pub(super) sex: Option<Sex>,
    pub(super) offer_slot_index: Option<u16>,
    pub(super) habitat: Entity,
    pub(super) transform: Transform,
}

#[derive(Component, Debug, Clone)]
pub(super) struct PendingAdoption {
    pub(super) species: AssetId,
    pub(super) variant: AssetId,
    pub(super) sex: Sex,
    pub(super) offer_slot_index: Option<u16>,
    pub(super) habitat: Entity,
    pub(super) transform: Transform,
    pub(super) cost: Money,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnimalAdopted {
    pub(crate) operation: Entity,
    pub(crate) animal: Entity,
    pub(crate) cost: Money,
    pub(crate) species: AssetId,
    pub(crate) sex: Sex,
    pub(crate) offer_slot_index: Option<u16>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeclineCurrentAnimalAdoptionOffers;
