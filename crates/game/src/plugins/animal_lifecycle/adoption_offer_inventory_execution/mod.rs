use bevy::prelude::*;
use openzt2_game_data::{
    species::{LifeStage, Sex, SpeciesFlags},
    AssetId,
};

use crate::assets::species::species_asset_types::SpeciesAsset;
use crate::assets::species::species_asset_types::SpeciesAssets;
use crate::assets::species::species_asset_types::SpeciesView;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::game_session_types::WorldSessionMode;
use crate::plugins::progression::adoption_and_content_availability_types::SpeciesAdoptionChanceMultiplier;
use crate::plugins::progression::fame_types::Fame;
use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;
use crate::plugins::simulation_time::deterministic_random_stream::RngDomain;
use crate::plugins::simulation_time::deterministic_random_stream::ZooSeed;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    adoption_offer_inventory_types::{
        AnimalAdoptionOffer, AnimalAdoptionOfferInventory, AnimalAdoptionOfferSlot,
    },
    animal_adoption_contracts::{AnimalAdopted, DeclineCurrentAnimalAdoptionOffers},
};

pub(super) fn initialize_animal_adoption_offer_inventory_from_loaded_authored_content(
    mut commands: Commands,
    zoo_seed: Option<Res<ZooSeed>>,
    fame: Res<Fame>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    world_roots: Query<
        (Entity, &SelectedWorldIdentity),
        (With<WorldRoot>, Without<AnimalAdoptionOfferInventory>),
    >,
    chance_multipliers: Query<&SpeciesAdoptionChanceMultiplier>,
) {
    if world_roots
        .iter()
        .all(|(_, world)| world.mode == WorldSessionMode::Freeform)
    {
        return;
    }
    let Some(zoo_seed) = zoo_seed else {
        return;
    };
    let (Some(species_view), Some(world_definitions)) = (
        species_index.get(&species_assets),
        active_world_definitions.get(&world_definition_assets),
    ) else {
        return;
    };
    let Some(configuration) = world_definitions.animal_adoption_offer_configuration() else {
        return;
    };
    let fame_percent = u16::from(fame.half_stars).saturating_mul(10);
    let slot_count = current_authored_animal_adoption_offer_slot_count(configuration, fame_percent);
    for (world_root, selected_world) in &world_roots {
        if selected_world.mode == WorldSessionMode::Freeform {
            continue;
        }
        let mut random = DeterministicRng::from_entity(
            *zoo_seed,
            PersistentId(0),
            RngDomain::AnimalAdoptionOffers,
        );
        let mut inventory = AnimalAdoptionOfferInventory {
            slots: vec![AnimalAdoptionOfferSlot::default(); usize::from(slot_count)],
            random_state_and_stream: random.to_raw(),
        };
        populate_vacant_animal_adoption_offer_slots(
            &mut inventory,
            species_view,
            animal_adoption_offer_fame_threshold_for_world_session_mode(
                fame_percent,
                selected_world.mode,
            ),
            &chance_multipliers,
            &mut random,
            true,
            None,
        );
        inventory.random_state_and_stream = random.to_raw();
        commands.entity(world_root).insert(inventory);
    }
}

pub(super) fn advance_and_repopulate_animal_adoption_offer_inventory(
    fixed_time: Res<Time<Fixed>>,
    fame: Res<Fame>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    chance_multipliers: Query<&SpeciesAdoptionChanceMultiplier>,
    mut decline_requests: MessageReader<DeclineCurrentAnimalAdoptionOffers>,
    mut inventories: Query<
        (&mut AnimalAdoptionOfferInventory, &SelectedWorldIdentity),
        With<WorldRoot>,
    >,
    mut species_with_native_adoption_profile_pairs: Local<Option<Vec<AssetId>>>,
) {
    let (Some(species_view), Some(world_definitions)) = (
        species_index.get(&species_assets),
        active_world_definitions.get(&world_definition_assets),
    ) else {
        return;
    };
    let Some(configuration) = world_definitions.animal_adoption_offer_configuration() else {
        return;
    };
    if species_with_native_adoption_profile_pairs.is_none()
        || species_assets.is_changed()
        || species_index.is_changed()
    {
        *species_with_native_adoption_profile_pairs = Some(
            species_view
                .species()
                .filter(|species_record| {
                    species_has_native_adoption_profile_pair_with_a_nonzero_authored_count(
                        species_view,
                        species_record.id,
                    )
                })
                .map(|species_record| species_record.id)
                .collect(),
        );
    }
    let declined = decline_requests.read().next().is_some();
    let elapsed_seconds = fixed_time.delta_secs().max(0.0);
    let fame_percent = u16::from(fame.half_stars).saturating_mul(10);
    let slot_count = usize::from(current_authored_animal_adoption_offer_slot_count(
        configuration,
        fame_percent,
    ));
    for (mut inventory, selected_world) in &mut inventories {
        inventory
            .slots
            .resize(slot_count, AnimalAdoptionOfferSlot::default());
        if declined {
            for slot in &mut inventory.slots {
                for option_index in 0..slot.offers.len() {
                    slot.repopulate_after_seconds[option_index] = slot.offers[option_index]
                        .take()
                        .map_or(0.0, |offer| offer.dismiss_cooldown_seconds);
                }
            }
        }
        for slot in &mut inventory.slots {
            for option_index in 0..slot.offers.len() {
                let mut remove_expired_offer = false;
                if let Some(offer) = slot.offers[option_index].as_mut() {
                    if offer.unlock_remaining_seconds > 0.0 {
                        offer.unlock_remaining_seconds =
                            (offer.unlock_remaining_seconds - elapsed_seconds).max(0.0);
                    }
                    if let Some(remove_remaining_seconds) = offer.remove_remaining_seconds.as_mut()
                    {
                        *remove_remaining_seconds -= elapsed_seconds;
                        if *remove_remaining_seconds <= 0.0 {
                            remove_expired_offer = true;
                        }
                    }
                } else {
                    slot.repopulate_after_seconds[option_index] =
                        (slot.repopulate_after_seconds[option_index] - elapsed_seconds).max(0.0);
                }
                if remove_expired_offer {
                    slot.offers[option_index] = None;
                }
            }
        }
        let mut random = DeterministicRng::from_raw(inventory.random_state_and_stream);
        populate_vacant_animal_adoption_offer_slots(
            &mut inventory,
            species_view,
            animal_adoption_offer_fame_threshold_for_world_session_mode(
                fame_percent,
                selected_world.mode,
            ),
            &chance_multipliers,
            &mut random,
            false,
            species_with_native_adoption_profile_pairs.as_deref(),
        );
        inventory.random_state_and_stream = random.to_raw();
    }
}

const fn animal_adoption_offer_fame_threshold_for_world_session_mode(
    fame_percent: u16,
    mode: WorldSessionMode,
) -> u16 {
    if matches!(mode, WorldSessionMode::Freeform) {
        u16::MAX
    } else {
        fame_percent
    }
}

fn current_authored_animal_adoption_offer_slot_count(
    configuration: &openzt2_game_data::world_definitions::catalogue_and_progression::animal_adoption_offer_definition_types::AnimalAdoptionOfferConfiguration,
    fame_percent: u16,
) -> u16 {
    configuration
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
        )
}

pub(super) fn consume_completed_animal_adoption_offer_counts(
    mut adopted_animals: MessageReader<AnimalAdopted>,
    mut inventories: Query<&mut AnimalAdoptionOfferInventory, With<WorldRoot>>,
) {
    for adopted in adopted_animals.read() {
        let Some(slot_index) = adopted.offer_slot_index.map(usize::from) else {
            continue;
        };
        for mut inventory in &mut inventories {
            let Some(slot) = inventory.slots.get_mut(slot_index) else {
                continue;
            };
            let Some(option_index) = slot
                .offers
                .iter()
                .position(|offer| offer.is_some_and(|offer| offer.species == adopted.species))
            else {
                continue;
            };
            let exhausted_offer_cooldown = {
                let Some(offer) = slot.offers[option_index].as_mut() else {
                    continue;
                };
                let count = match adopted.sex {
                    Sex::Female => &mut offer.remaining_by_sex[0],
                    Sex::Male => &mut offer.remaining_by_sex[1],
                    Sex::Any => continue,
                };
                *count = count.saturating_sub(1);
                (offer.remaining_by_sex == [0, 0]).then_some(offer.dismiss_cooldown_seconds)
            };
            if let Some(dismiss_cooldown_seconds) = exhausted_offer_cooldown {
                slot.repopulate_after_seconds[option_index] = dismiss_cooldown_seconds;
                slot.offers[option_index] = None;
            }
        }
    }
}

fn populate_vacant_animal_adoption_offer_slots(
    inventory: &mut AnimalAdoptionOfferInventory,
    species_view: SpeciesView<'_>,
    fame_percent: u16,
    chance_multipliers: &Query<&SpeciesAdoptionChanceMultiplier>,
    random: &mut DeterministicRng,
    first_population: bool,
    species_with_native_adoption_profile_pairs: Option<&[AssetId]>,
) {
    let mut candidates = species_view
        .species()
        .filter(|species| {
            species.flags.contains(SpeciesFlags::ADOPTABLE)
                && species.adoption_offer.rarity_fame_percent <= fame_percent
                && species_with_native_adoption_profile_pairs.map_or_else(
                    || {
                        species_has_native_adoption_profile_pair_with_a_nonzero_authored_count(
                            species_view,
                            species.id,
                        )
                    },
                    |species_ids| species_ids.contains(&species.id),
                )
                && !inventory.slots.iter().any(|slot| {
                    slot.offers
                        .iter()
                        .any(|offer| offer.is_some_and(|offer| offer.species == species.id))
                })
        })
        .collect::<Vec<_>>();
    for slot_index in 0..inventory.slots.len() {
        for option_index in 0..inventory.slots[slot_index].offers.len() {
            if inventory.slots[slot_index].offers[option_index].is_some()
                || inventory.slots[slot_index].repopulate_after_seconds[option_index] > 0.0
                || candidates.is_empty()
            {
                continue;
            }
            let total_weight = candidates
                .iter()
                .map(|species| adoption_offer_weight(species.id, chance_multipliers))
                .fold(0_u32, u32::saturating_add);
            let Some(mut selected_weight) = random.range_u32(total_weight) else {
                break;
            };
            let selected_index = candidates
                .iter()
                .position(|species| {
                    let weight = adoption_offer_weight(species.id, chance_multipliers);
                    if selected_weight < weight {
                        true
                    } else {
                        selected_weight -= weight;
                        false
                    }
                })
                .unwrap_or_default();
            let selected = candidates.swap_remove(selected_index);
            let remaining_by_sex = [Sex::Female, Sex::Male].map(|sex| {
                species_view
                    .variants(selected.id)
                    .find(|variant| variant.life_stage == LifeStage::Adult && variant.sex == sex)
                    .map_or(0, |variant| {
                        sample_inclusive_u16_range(random, variant.adoption_count_range)
                    })
            });
            if remaining_by_sex == [0, 0] {
                continue;
            }
            let unlock_remaining_seconds = if first_population {
                0.0
            } else {
                sample_inclusive_f32_range(random, selected.adoption_offer.unlock_seconds_range)
            };
            inventory.slots[slot_index].offers[option_index] = Some(AnimalAdoptionOffer {
                species: selected.id,
                remaining_by_sex,
                unlock_remaining_seconds,
                remove_remaining_seconds: (selected.adoption_offer.remove_after_seconds > 0.0)
                    .then_some(selected.adoption_offer.remove_after_seconds),
                dismiss_cooldown_seconds: selected.adoption_offer.dismiss_cooldown_seconds,
            });
        }
    }
}

fn species_has_native_adoption_profile_pair_with_a_nonzero_authored_count(
    species_view: SpeciesView<'_>,
    species: AssetId,
) -> bool {
    let mut has_adult_female_profile = false;
    let mut has_adult_male_profile = false;
    let mut has_nonzero_authored_count = false;
    for variant in species_view
        .variants(species)
        .filter(|variant| variant.life_stage == LifeStage::Adult)
    {
        match variant.sex {
            Sex::Female => has_adult_female_profile = true,
            Sex::Male => has_adult_male_profile = true,
            Sex::Any => continue,
        }
        has_nonzero_authored_count |= variant.adoption_count_range[1] != 0;
    }
    has_adult_female_profile && has_adult_male_profile && has_nonzero_authored_count
}

fn adoption_offer_weight(
    species: AssetId,
    chance_multipliers: &Query<&SpeciesAdoptionChanceMultiplier>,
) -> u32 {
    chance_multipliers
        .iter()
        .find(|multiplier| multiplier.definition == species)
        .map_or(1_000, |multiplier| {
            u32::from(multiplier.multiplier_permille)
        })
}

fn sample_inclusive_u16_range(random: &mut DeterministicRng, range: [u16; 2]) -> u16 {
    let width = u32::from(range[1].saturating_sub(range[0])) + 1;
    range[0].saturating_add(random.range_u32(width).unwrap_or_default() as u16)
}

fn sample_inclusive_f32_range(random: &mut DeterministicRng, range: [f32; 2]) -> f32 {
    range[0] + (range[1] - range[0]).max(0.0) * random.unit_f32()
}
