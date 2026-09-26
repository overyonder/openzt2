use std::collections::HashSet;

use openzt2_game_data::AssetId;

use crate::plugins::animal_lifecycle::adoption_offer_inventory_types::{
    AnimalAdoptionOffer, AnimalAdoptionOfferInventory, AnimalAdoptionOfferSlot,
};
use crate::{
    assets::species::species_asset_types::SpeciesView, plugins::progression::fame_types::Fame,
};

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure;

const ANIMAL_ADOPTION_OFFER_INVENTORY_RANDOM_STATE_BYTE_COUNT: usize = 16;
const ANIMAL_ADOPTION_OFFER_SNAPSHOT_BYTE_COUNT: usize = 34;
const ANIMAL_ADOPTION_OFFER_SLOT_SNAPSHOT_BYTE_COUNT: usize =
    8 + 2 * ANIMAL_ADOPTION_OFFER_SNAPSHOT_BYTE_COUNT;

pub(super) fn append_encoded_animal_adoption_offer_inventory_snapshot_section(
    snapshot_section_bytes: &mut Vec<u8>,
    inventory: &AnimalAdoptionOfferInventory,
) {
    let slots = inventory.slots();
    let random_state_and_stream = inventory.random_state_and_stream();
    random_state_and_stream
        .iter()
        .for_each(|value| snapshot_section_bytes.extend_from_slice(&value.to_le_bytes()));
    for slot in slots {
        let offers = slot.offers();
        let repopulate_after_seconds = slot.repopulate_after_seconds();
        repopulate_after_seconds.iter().for_each(|seconds| {
            snapshot_section_bytes.extend_from_slice(&seconds.to_le_bytes());
        });
        offers.iter().for_each(|offer| {
            snapshot_section_bytes.push(u8::from(offer.is_some()));
            let species = offer.map_or(AssetId::default(), AnimalAdoptionOffer::species);
            let remaining_by_sex = offer.map_or([0; 2], AnimalAdoptionOffer::remaining_by_sex);
            let unlock_remaining_seconds =
                offer.map_or(0.0, AnimalAdoptionOffer::unlock_remaining_seconds);
            let remove_remaining_seconds =
                offer.and_then(AnimalAdoptionOffer::remove_remaining_seconds);
            let dismiss_cooldown_seconds =
                offer.map_or(0.0, AnimalAdoptionOffer::dismiss_cooldown_seconds);
            snapshot_section_bytes.extend_from_slice(&species.0);
            remaining_by_sex.iter().for_each(|remaining| {
                snapshot_section_bytes.extend_from_slice(&remaining.to_le_bytes());
            });
            snapshot_section_bytes.extend_from_slice(&unlock_remaining_seconds.to_le_bytes());
            snapshot_section_bytes.push(u8::from(remove_remaining_seconds.is_some()));
            snapshot_section_bytes
                .extend_from_slice(&remove_remaining_seconds.unwrap_or_default().to_le_bytes());
            snapshot_section_bytes.extend_from_slice(&dismiss_cooldown_seconds.to_le_bytes());
        });
    }
}

pub(super) fn decode_and_validate_animal_adoption_offer_inventory_snapshot_section(
    snapshot_section_bytes: &[u8],
    declared_slot_record_count: u32,
) -> Result<AnimalAdoptionOfferInventory, WorldSnapshotPersistenceFailure> {
    let slot_count = usize::try_from(declared_slot_record_count)
        .map_err(|_| WorldSnapshotPersistenceFailure::CapacityExceeded)?;
    if slot_count > usize::from(u16::MAX)
        || snapshot_section_bytes.len()
            != ANIMAL_ADOPTION_OFFER_INVENTORY_RANDOM_STATE_BYTE_COUNT
                .checked_add(
                    slot_count.saturating_mul(ANIMAL_ADOPTION_OFFER_SLOT_SNAPSHOT_BYTE_COUNT),
                )
                .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?
    {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }

    let mut reader = AnimalAdoptionOfferInventorySnapshotSectionReader::new(snapshot_section_bytes);
    let random_state_and_stream = [reader.read_u64()?, reader.read_u64()?];
    if random_state_and_stream[1] & 1 == 0 {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    let mut encountered_species = HashSet::with_capacity(slot_count.saturating_mul(2));
    let slots = (0..slot_count)
        .map(|_| {
            let repopulate_after_seconds = [reader.read_f32()?, reader.read_f32()?];
            if repopulate_after_seconds
                .iter()
                .any(|seconds| !seconds.is_finite() || *seconds < 0.0)
            {
                return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
            }
            let offers = [0, 1].map(|option_index| {
                let present = reader.read_boolean()?;
                let species = reader.read_asset_identifier()?;
                let remaining_by_sex = [reader.read_u16()?, reader.read_u16()?];
                let unlock_remaining_seconds = reader.read_f32()?;
                let has_remove_remaining_seconds = reader.read_boolean()?;
                let encoded_remove_remaining_seconds = reader.read_f32()?;
                let dismiss_cooldown_seconds = reader.read_f32()?;
                if !present {
                    if species != AssetId::default()
                        || remaining_by_sex != [0, 0]
                        || unlock_remaining_seconds != 0.0
                        || has_remove_remaining_seconds
                        || encoded_remove_remaining_seconds != 0.0
                        || dismiss_cooldown_seconds != 0.0
                    {
                        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
                    }
                    return Ok(None);
                }
                let remove_remaining_seconds =
                    has_remove_remaining_seconds.then_some(encoded_remove_remaining_seconds);
                if species == AssetId::default()
                    || remaining_by_sex == [0, 0]
                    || !unlock_remaining_seconds.is_finite()
                    || unlock_remaining_seconds < 0.0
                    || remove_remaining_seconds
                        .is_some_and(|seconds| !seconds.is_finite() || seconds <= 0.0)
                    || !dismiss_cooldown_seconds.is_finite()
                    || dismiss_cooldown_seconds < 0.0
                    || repopulate_after_seconds[option_index] != 0.0
                    || !encountered_species.insert(species)
                {
                    return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
                }
                Ok(Some(
                    AnimalAdoptionOffer::from_restored_adoption_offer_state(
                        species,
                        remaining_by_sex,
                        unlock_remaining_seconds,
                        remove_remaining_seconds,
                        dismiss_cooldown_seconds,
                    ),
                ))
            });
            let [first_offer, second_offer] = offers;
            Ok(
                AnimalAdoptionOfferSlot::from_restored_adoption_offer_slot_state(
                    [first_offer?, second_offer?],
                    repopulate_after_seconds,
                ),
            )
        })
        .collect::<Result<Vec<_>, WorldSnapshotPersistenceFailure>>()?;
    if reader.remaining_byte_count() != 0 {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    Ok(
        AnimalAdoptionOfferInventory::from_restored_adoption_offer_inventory_state(
            slots,
            random_state_and_stream,
        ),
    )
}

pub(super) fn validate_animal_adoption_offer_inventory_against_loaded_authored_content(
    inventory: &AnimalAdoptionOfferInventory,
    species: SpeciesView<'_>,
    configuration: &openzt2_game_data::world_definitions::catalogue_and_progression::animal_adoption_offer_definition_types::AnimalAdoptionOfferConfiguration,
    fame: Fame,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    let fame_percent = u16::from(fame.half_stars).saturating_mul(10);
    let expected_slot_count = configuration
        .base_slot_count
        .saturating_add(configuration.installed_expansion_slot_count)
        .saturating_add(
            configuration
                .fame_slots
                .iter()
                .filter(|threshold| threshold.fame_percent <= fame_percent)
                .map(|threshold| threshold.additional_slot_count)
                .max()
                .unwrap_or_default(),
        );
    let slots = inventory.slots();
    if slots.len() != usize::from(expected_slot_count) {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    if slots.iter().any(|slot| {
        slot.offers()
            .into_iter()
            .flatten()
            .any(|offer| species.find(offer.species()).is_none())
    }) {
        return Err(WorldSnapshotPersistenceFailure::UnknownAssetIdentifier);
    }
    Ok(())
}

struct AnimalAdoptionOfferInventorySnapshotSectionReader<'a> {
    bytes: &'a [u8],
    byte_offset: usize,
}

impl<'a> AnimalAdoptionOfferInventorySnapshotSectionReader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            byte_offset: 0,
        }
    }

    fn read_fixed_byte_array<const BYTE_COUNT: usize>(
        &mut self,
    ) -> Result<[u8; BYTE_COUNT], WorldSnapshotPersistenceFailure> {
        let end_byte_offset = self
            .byte_offset
            .checked_add(BYTE_COUNT)
            .ok_or(WorldSnapshotPersistenceFailure::CapacityExceeded)?;
        let bytes = self
            .bytes
            .get(self.byte_offset..end_byte_offset)
            .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?
            .try_into()
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        self.byte_offset = end_byte_offset;
        Ok(bytes)
    }

    fn read_boolean(&mut self) -> Result<bool, WorldSnapshotPersistenceFailure> {
        match self.read_fixed_byte_array::<1>()?[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection),
        }
    }

    fn read_u16(&mut self) -> Result<u16, WorldSnapshotPersistenceFailure> {
        Ok(u16::from_le_bytes(self.read_fixed_byte_array()?))
    }

    fn read_u64(&mut self) -> Result<u64, WorldSnapshotPersistenceFailure> {
        Ok(u64::from_le_bytes(self.read_fixed_byte_array()?))
    }

    fn read_f32(&mut self) -> Result<f32, WorldSnapshotPersistenceFailure> {
        Ok(f32::from_le_bytes(self.read_fixed_byte_array()?))
    }

    fn read_asset_identifier(&mut self) -> Result<AssetId, WorldSnapshotPersistenceFailure> {
        Ok(AssetId(self.read_fixed_byte_array()?))
    }

    fn remaining_byte_count(&self) -> usize {
        self.bytes.len().saturating_sub(self.byte_offset)
    }
}
