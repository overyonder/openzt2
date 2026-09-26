use bevy::prelude::*;
use openzt2_game_data::{species::Sex, AssetId};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AnimalAdoptionOffer {
    pub(super) species: AssetId,
    pub(super) remaining_by_sex: [u16; 2],
    pub(super) unlock_remaining_seconds: f32,
    pub(super) remove_remaining_seconds: Option<f32>,
    pub(super) dismiss_cooldown_seconds: f32,
}

impl AnimalAdoptionOffer {
    pub(crate) const fn from_restored_adoption_offer_state(
        species: AssetId,
        remaining_by_sex: [u16; 2],
        unlock_remaining_seconds: f32,
        remove_remaining_seconds: Option<f32>,
        dismiss_cooldown_seconds: f32,
    ) -> Self {
        Self {
            species,
            remaining_by_sex,
            unlock_remaining_seconds,
            remove_remaining_seconds,
            dismiss_cooldown_seconds,
        }
    }

    pub(crate) const fn remaining_by_sex(self) -> [u16; 2] {
        self.remaining_by_sex
    }

    pub(crate) const fn unlock_remaining_seconds(self) -> f32 {
        self.unlock_remaining_seconds
    }

    pub(crate) const fn remove_remaining_seconds(self) -> Option<f32> {
        self.remove_remaining_seconds
    }

    pub(crate) const fn dismiss_cooldown_seconds(self) -> f32 {
        self.dismiss_cooldown_seconds
    }

    pub(crate) fn species(self) -> AssetId {
        self.species
    }

    pub(crate) fn remaining_for_sex(self, sex: Sex) -> u16 {
        match sex {
            Sex::Female => self.remaining_by_sex[0],
            Sex::Male => self.remaining_by_sex[1],
            Sex::Any => self.remaining_by_sex.into_iter().sum(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct AnimalAdoptionOfferSlot {
    pub(super) offers: [Option<AnimalAdoptionOffer>; 2],
    pub(super) repopulate_after_seconds: [f32; 2],
}

impl AnimalAdoptionOfferSlot {
    pub(crate) const fn from_restored_adoption_offer_slot_state(
        offers: [Option<AnimalAdoptionOffer>; 2],
        repopulate_after_seconds: [f32; 2],
    ) -> Self {
        Self {
            offers,
            repopulate_after_seconds,
        }
    }

    pub(crate) const fn offers(self) -> [Option<AnimalAdoptionOffer>; 2] {
        self.offers
    }

    pub(crate) const fn repopulate_after_seconds(self) -> [f32; 2] {
        self.repopulate_after_seconds
    }
}

#[derive(Component, Debug, PartialEq)]
pub(crate) struct AnimalAdoptionOfferInventory {
    pub(super) slots: Vec<AnimalAdoptionOfferSlot>,
    pub(super) random_state_and_stream: [u64; 2],
}

impl AnimalAdoptionOfferInventory {
    pub(crate) fn from_restored_adoption_offer_inventory_state(
        slots: Vec<AnimalAdoptionOfferSlot>,
        random_state_and_stream: [u64; 2],
    ) -> Self {
        Self {
            slots,
            random_state_and_stream,
        }
    }

    pub(crate) fn slots(&self) -> &[AnimalAdoptionOfferSlot] {
        &self.slots
    }

    pub(crate) const fn random_state_and_stream(&self) -> [u64; 2] {
        self.random_state_and_stream
    }

    pub(crate) fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub(crate) fn offer_at_slot(
        &self,
        slot_index: usize,
        option_index: usize,
    ) -> Option<&AnimalAdoptionOffer> {
        self.slots
            .get(slot_index)?
            .offers
            .get(option_index)?
            .as_ref()
    }

    pub(crate) fn actionable_offers(
        &self,
        sex: Sex,
    ) -> impl Iterator<Item = (u16, &AnimalAdoptionOffer)> {
        self.slots
            .iter()
            .enumerate()
            .flat_map(move |(index, slot)| {
                slot.offers.iter().filter_map(move |offer| {
                    let offer = offer.as_ref()?;
                    (offer.unlock_remaining_seconds <= 0.0 && offer.remaining_for_sex(sex) != 0)
                        .then_some((index.min(usize::from(u16::MAX)) as u16, offer))
                })
            })
    }

    pub(crate) fn offer_is_actionable(
        &self,
        slot_index: usize,
        species: AssetId,
        sex: Sex,
    ) -> bool {
        self.slots.get(slot_index).is_some_and(|slot| {
            slot.offers.into_iter().flatten().any(|offer| {
                offer.species == species
                    && offer.unlock_remaining_seconds <= 0.0
                    && offer.remaining_for_sex(sex) != 0
            })
        })
    }
}
