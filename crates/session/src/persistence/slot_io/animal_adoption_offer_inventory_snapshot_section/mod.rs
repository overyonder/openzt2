use std::collections::HashSet;

use openzt2_game_data::AssetId;

use bevy::{
    ecs::{relationship::RelationshipTarget, system::SystemParam},
    prelude::*,
};
use openzt2_animals::animal_lifecycle::adoption_offer_types::{
    AdoptionOfferClock, AdoptionOfferCooldown, AdoptionOfferForOption, AdoptionOfferOption,
    AdoptionOfferOptions, AdoptionOfferRandom, AdoptionOfferSupply, CurrentAdoptionOffer,
};
use openzt2_game_assets::species::species_asset_types::SpeciesView;
use openzt2_world_state::game_session_types::WorldSessionMode;
use openzt2_world_state::{
    simulation_time::deterministic_random_stream::DeterministicRng,
    world_spawn::world_membership_types::{WorldMember, WorldRoot},
};
use openzt2_zoo_state::animal_lifecycle::animal_adoption_contracts::PendingAdoption;
use openzt2_zoo_state::progression::fame_types::Fame;

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure;

const ANIMAL_ADOPTION_OFFER_INVENTORY_RANDOM_STATE_BYTE_COUNT: usize = 16;
const ANIMAL_ADOPTION_OFFER_SNAPSHOT_BYTE_COUNT: usize = 34;
const ANIMAL_ADOPTION_OFFER_SLOT_SNAPSHOT_BYTE_COUNT: usize =
    8 + 2 * ANIMAL_ADOPTION_OFFER_SNAPSHOT_BYTE_COUNT;

#[derive(Debug, PartialEq)]
pub(super) struct AnimalAdoptionOffersSnapshot {
    random_state_and_stream: [u64; 2],
    slots: Vec<AdoptionOfferSlotSnapshot>,
}

impl AnimalAdoptionOffersSnapshot {
    pub(super) fn slot_count(&self) -> usize {
        self.slots.len()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct AdoptionOfferSlotSnapshot {
    repopulate_after_seconds: [f32; 2],
    offers: [Option<AdoptionOfferSnapshot>; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AdoptionOfferSnapshot {
    species: AssetId,
    remaining_by_sex: [u16; 2],
    unlock_remaining_seconds: f32,
    remove_remaining_seconds: Option<f32>,
    dismiss_cooldown_seconds: f32,
}

#[derive(SystemParam)]
pub(super) struct AdoptionOfferSnapshotCapture<'w, 's> {
    roots: Query<
        'w,
        's,
        (
            Entity,
            Option<&'static AdoptionOfferClock>,
            Option<&'static AdoptionOfferRandom>,
            Option<&'static AdoptionOfferOptions>,
        ),
        With<WorldRoot>,
    >,
    options: Query<
        'w,
        's,
        (
            &'static AdoptionOfferOption,
            Option<&'static AdoptionOfferCooldown>,
            Option<&'static CurrentAdoptionOffer>,
        ),
    >,
    supplies: Query<'w, 's, &'static AdoptionOfferSupply>,
    adoptions: Query<'w, 's, &'static PendingAdoption>,
}

impl AdoptionOfferSnapshotCapture<'_, '_> {
    pub(super) fn capture(
        &self,
    ) -> Result<Option<AnimalAdoptionOffersSnapshot>, WorldSnapshotPersistenceFailure> {
        let invalid = WorldSnapshotPersistenceFailure::CorruptSnapshotSection;
        let (root, clock, random, options) = self.roots.single().map_err(|_| invalid)?;
        let (clock, random) = match (clock, random, options) {
            (None, None, None) => return Ok(None),
            (Some(clock), Some(random), _) => (clock.0, random.0.to_raw()),
            _ => return Err(invalid),
        };
        if self
            .adoptions
            .iter()
            .any(|adoption| adoption.world == root && adoption.offer_slot_index.is_some())
        {
            return Err(WorldSnapshotPersistenceFailure::SnapshotTransactionInFlight);
        }

        let members = options.into_iter().flat_map(RelationshipTarget::iter);
        let option_count = options.map_or(0, |options| options.iter().count());
        if option_count % 2 != 0 || option_count / 2 > usize::from(u16::MAX) {
            return Err(invalid);
        }
        let mut slots = vec![AdoptionOfferSlotSnapshot::default(); option_count / 2];
        let mut seen_options = vec![[false; 2]; slots.len()];
        let remaining = |deadline: f64| (deadline - clock).max(0.0) as f32;
        for entity in members {
            let (option, cooldown, current) = self.options.get(entity).map_err(|_| invalid)?;
            let slot_index = usize::from(option.slot);
            let option_index = usize::from(option.option);
            let slot = slots.get_mut(slot_index).ok_or(invalid)?;
            let seen = seen_options[slot_index]
                .get_mut(option_index)
                .ok_or(invalid)?;
            if std::mem::replace(seen, true) || option.world != root {
                return Err(invalid);
            }
            if let Some(current) = current {
                let supply = self
                    .supplies
                    .get(*current.collection())
                    .map_err(|_| invalid)?;
                if supply.available_by_sex == [0; 2] {
                    return Err(WorldSnapshotPersistenceFailure::SnapshotTransactionInFlight);
                }
                slot.offers[option_index] = Some(AdoptionOfferSnapshot {
                    species: supply.species,
                    remaining_by_sex: supply.available_by_sex,
                    unlock_remaining_seconds: remaining(supply.unlock_at),
                    remove_remaining_seconds: supply
                        .remove_at
                        .map(remaining)
                        .filter(|seconds| *seconds > 0.0),
                    dismiss_cooldown_seconds: supply.dismiss_cooldown,
                });
                if supply.remove_at.is_some()
                    && slot.offers[option_index]
                        .is_some_and(|offer| offer.remove_remaining_seconds.is_none())
                {
                    return Err(WorldSnapshotPersistenceFailure::SnapshotTransactionInFlight);
                }
            } else if let Some(cooldown) = cooldown {
                slot.repopulate_after_seconds[option_index] = remaining(cooldown.0);
            }
        }
        Ok(Some(AnimalAdoptionOffersSnapshot {
            random_state_and_stream: random,
            slots,
        }))
    }
}

pub(super) fn spawn_saved_adoption_offers(
    commands: &mut Commands,
    root: Entity,
    snapshot: AnimalAdoptionOffersSnapshot,
) {
    commands.entity(root).insert((
        AdoptionOfferClock(0.0),
        AdoptionOfferRandom(DeterministicRng::from_raw(snapshot.random_state_and_stream)),
    ));
    for (slot, state) in snapshot.slots.into_iter().enumerate() {
        for option in 0..2u8 {
            let mut entity = commands.spawn(AdoptionOfferOption {
                world: root,
                slot: slot as u16,
                option,
            });
            let index = usize::from(option);
            if let Some(offer) = state.offers[index] {
                let option_entity = entity.id();
                commands.spawn((
                    WorldMember { root },
                    AdoptionOfferForOption(option_entity),
                    AdoptionOfferSupply::new(
                        offer.species,
                        offer.remaining_by_sex,
                        f64::from(offer.unlock_remaining_seconds),
                        offer.remove_remaining_seconds.map(f64::from),
                        offer.dismiss_cooldown_seconds,
                    ),
                ));
            } else if state.repopulate_after_seconds[index] > 0.0 {
                entity.insert(AdoptionOfferCooldown(f64::from(
                    state.repopulate_after_seconds[index],
                )));
            }
        }
    }
}

pub(super) fn append_encoded_animal_adoption_offer_inventory_snapshot_section(
    snapshot_section_bytes: &mut Vec<u8>,
    inventory: Option<&AnimalAdoptionOffersSnapshot>,
) {
    // The section envelope distinguishes absence from a present zero-slot
    // inventory: absence has no payload, while presence retains its RNG state.
    let Some(inventory) = inventory else {
        return;
    };
    let slots = &inventory.slots;
    let random_state_and_stream = inventory.random_state_and_stream;
    random_state_and_stream
        .iter()
        .for_each(|value| snapshot_section_bytes.extend_from_slice(&value.to_le_bytes()));
    for slot in slots {
        let offers = slot.offers;
        let repopulate_after_seconds = slot.repopulate_after_seconds;
        repopulate_after_seconds.iter().for_each(|seconds| {
            snapshot_section_bytes.extend_from_slice(&seconds.to_le_bytes());
        });
        offers.iter().for_each(|offer| {
            snapshot_section_bytes.push(u8::from(offer.is_some()));
            let species = offer.map_or(AssetId::default(), |offer| offer.species);
            let remaining_by_sex = offer.map_or([0; 2], |offer| offer.remaining_by_sex);
            let unlock_remaining_seconds =
                offer.map_or(0.0, |offer| offer.unlock_remaining_seconds);
            let remove_remaining_seconds = offer.and_then(|offer| offer.remove_remaining_seconds);
            let dismiss_cooldown_seconds =
                offer.map_or(0.0, |offer| offer.dismiss_cooldown_seconds);
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
    session_mode: WorldSessionMode,
) -> Result<Option<AnimalAdoptionOffersSnapshot>, WorldSnapshotPersistenceFailure> {
    if snapshot_section_bytes.is_empty() {
        if declared_slot_record_count != 0 {
            return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
        }
        validate_animal_adoption_offer_inventory_presence_for_session_mode(None, session_mode)?;
        return Ok(None);
    }
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
                Ok(Some(AdoptionOfferSnapshot {
                    species,
                    remaining_by_sex,
                    unlock_remaining_seconds,
                    remove_remaining_seconds,
                    dismiss_cooldown_seconds,
                }))
            });
            let [first_offer, second_offer] = offers;
            Ok(AdoptionOfferSlotSnapshot {
                offers: [first_offer?, second_offer?],
                repopulate_after_seconds,
            })
        })
        .collect::<Result<Vec<_>, WorldSnapshotPersistenceFailure>>()?;
    if reader.remaining_byte_count() != 0 {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    Ok(Some(AnimalAdoptionOffersSnapshot {
        slots,
        random_state_and_stream,
    }))
}

pub(super) fn validate_animal_adoption_offer_inventory_presence_for_session_mode(
    inventory: Option<&AnimalAdoptionOffersSnapshot>,
    session_mode: WorldSessionMode,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    match (session_mode, inventory) {
        (WorldSessionMode::Freeform, _) | (_, Some(_)) => Ok(()),
        (WorldSessionMode::Challenge | WorldSessionMode::Campaign, None) => {
            Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)
        }
    }
}

pub(super) fn validate_animal_adoption_offer_inventory_against_loaded_authored_content(
    inventory: &AnimalAdoptionOffersSnapshot,
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
    let slots = &inventory.slots;
    if slots.len() != usize::from(expected_slot_count) {
        return Err(WorldSnapshotPersistenceFailure::CorruptSnapshotSection);
    }
    if slots.iter().any(|slot| {
        slot.offers
            .into_iter()
            .flatten()
            .any(|offer| species.find(offer.species).is_none())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_twelve_offer_bytes_preserve_stock_timers_and_random_state() {
        // Independently specified little-endian payload: one populated option,
        // one vacant option waiting 2.5 seconds, and the existing RNG header.
        let mut payload = vec![
            1, 2, 3, 4, 5, 6, 7, 8, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 64, 1,
        ];
        payload.extend_from_slice(&[0x11; 16]);
        payload.extend_from_slice(&[3, 0, 5, 0, 0, 0, 192, 63, 1, 0, 0, 200, 64, 0, 0, 128, 62]);
        payload.extend_from_slice(&[0; 34]);
        let expected = AnimalAdoptionOffersSnapshot {
            slots: vec![AdoptionOfferSlotSnapshot {
                offers: [
                    Some(AdoptionOfferSnapshot {
                        species: AssetId([0x11; 16]),
                        remaining_by_sex: [3, 5],
                        unlock_remaining_seconds: 1.5,
                        remove_remaining_seconds: Some(6.25),
                        dismiss_cooldown_seconds: 0.25,
                    }),
                    None,
                ],
                repopulate_after_seconds: [0.0, 2.5],
            }],
            random_state_and_stream: [0x0807_0605_0403_0201, 3],
        };
        for mode in [
            WorldSessionMode::Freeform,
            WorldSessionMode::Challenge,
            WorldSessionMode::Campaign,
        ] {
            let decoded = decode_and_validate_animal_adoption_offer_inventory_snapshot_section(
                &payload, 1, mode,
            )
            .unwrap();
            assert_eq!(decoded.as_ref(), Some(&expected));
            let mut encoded = Vec::new();
            append_encoded_animal_adoption_offer_inventory_snapshot_section(
                &mut encoded,
                decoded.as_ref(),
            );
            assert_eq!(encoded, payload);
        }

        // The same independently specified RNG header represents a present
        // zero-slot inventory; it must not become the absent Freeform state.
        let zero_slots = decode_and_validate_animal_adoption_offer_inventory_snapshot_section(
            &payload[..16],
            0,
            WorldSessionMode::Freeform,
        )
        .unwrap();
        let expected_zero_slots = AnimalAdoptionOffersSnapshot {
            slots: Vec::new(),
            random_state_and_stream: [0x0807_0605_0403_0201, 3],
        };
        assert_eq!(zero_slots.as_ref(), Some(&expected_zero_slots));
        let mut encoded_zero_slots = Vec::new();
        append_encoded_animal_adoption_offer_inventory_snapshot_section(
            &mut encoded_zero_slots,
            zero_slots.as_ref(),
        );
        assert_eq!(encoded_zero_slots, payload[..16]);
    }
}
