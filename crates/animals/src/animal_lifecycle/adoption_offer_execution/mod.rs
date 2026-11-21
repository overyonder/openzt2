use bevy::{
    ecs::{relationship::RelationshipTarget, system::SystemParam},
    platform::collections::{HashMap, HashSet},
    prelude::*,
};
use openzt2_game_data::{
    species::{LifeStage, Sex, SpeciesFlags},
    AssetId,
};

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_game_assets::species::species_asset_types::{SpeciesAsset, SpeciesAssets, SpeciesView};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::world_definitions::catalogue_and_progression::animal_adoption_offer_definition_types::AnimalAdoptionOfferConfiguration;
use openzt2_world_state::application_schedule::{FixedGameSet, GameSet};
use openzt2_world_state::game_session_types::WorldSessionMode;
use openzt2_world_state::simulation_time::deterministic_random_stream::{DeterministicRng, RngDomain, ZooSeed};
use openzt2_world_state::simulation_time::simulation_control_queries::simulation_is_running;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::progression::adoption_and_content_availability_types::SpeciesAdoptionChanceMultiplier;
use openzt2_zoo_state::progression::fame_types::Fame;

use super::adoption_offer_types::{
    AdoptionOfferClock, AdoptionOfferCooldown, AdoptionOfferForOption, AdoptionOfferOption,
    AdoptionOfferOptions, AdoptionOfferRandom, AdoptionOfferSupply, AdoptionOfferUnlocked,
    CurrentAdoptionOffer,
};
use super::animal_adoption_contracts::DeclineCurrentAnimalAdoptionOffers;

#[derive(Component)]
struct InitialAdoptionOfferPopulation;

#[derive(Component)]
pub(super) struct AdoptionOfferClaim(Option<(Entity, usize)>);

impl AdoptionOfferClaim {
    pub(super) fn settled(&self) -> bool {
        self.0.is_none()
    }
}

#[derive(SystemParam)]
pub(super) struct AdoptionOfferClaims<'w, 's> {
    worlds: Query<'w, 's, (&'static AdoptionOfferClock, &'static AdoptionOfferOptions)>,
    options: Query<'w, 's, (&'static AdoptionOfferOption, &'static CurrentAdoptionOffer)>,
    offers: Query<
        'w,
        's,
        (
            &'static mut AdoptionOfferSupply,
            Option<&'static AdoptionOfferForOption>,
        ),
    >,
}

impl AdoptionOfferClaims<'_, '_> {
    pub(super) fn reserve(
        &mut self,
        world: Entity,
        slot: u16,
        species: AssetId,
        sex: Sex,
    ) -> Option<AdoptionOfferClaim> {
        let sex_index = match sex {
            Sex::Female => 0,
            Sex::Male => 1,
            Sex::Any => return None,
        };
        let (clock, options) = self.worlds.get(world).ok()?;
        let offer = self
            .options
            .iter_many(options.iter())
            .flatten()
            .filter(|(option, _)| option.slot == slot)
            .find_map(|(_, current)| {
                let offer = *current.collection();
                let (supply, _) = self.offers.get(offer).ok()?;
                (supply.species == species
                    && supply.unlock_at <= clock.0
                    && supply.available_by_sex[sex_index] != 0)
                    .then_some(offer)
            })?;
        self.offers.get_mut(offer).ok()?.0.reserve(sex_index);
        Some(AdoptionOfferClaim(Some((offer, sex_index))))
    }

    pub(super) fn settle(
        &mut self,
        claim: &mut AdoptionOfferClaim,
        purchased: bool,
        commands: &mut Commands,
    ) {
        let Some((offer, sex_index)) = claim.0.take() else {
            return;
        };
        let Ok((mut supply, option)) = self.offers.get_mut(offer) else {
            return;
        };
        supply.settle(sex_index, purchased);
        if supply.has_pending_claims() {
            return;
        }
        let Some((option_entity, option)) = option.and_then(|relation| {
            self.options
                .get(relation.0)
                .ok()
                .map(|(option, _)| (relation.0, option))
        }) else {
            commands.entity(offer).try_despawn();
            return;
        };
        if supply.available_by_sex == [0; 2] {
            if let Ok((clock, _)) = self.worlds.get(option.world) {
                commands.entity(option_entity).insert(AdoptionOfferCooldown(
                    clock.0 + f64::from(supply.dismiss_cooldown),
                ));
            }
            commands.entity(offer).try_despawn();
        }
    }
}

pub(super) fn register_adoption_offers(app: &mut App) {
    app.add_systems(
        Update,
        (
            initialize_adoption_offer_options,
            refill_adoption_offer_options::<true>,
        )
            .chain()
            .in_set(GameSet::Intent)
            .run_if(in_state(GamePhase::InGame)),
    )
    .add_systems(
        FixedUpdate,
        (
            resize_adoption_offer_options,
            advance_adoption_offer_deadlines,
            refill_adoption_offer_options::<false>.run_if(adoption_options_are_fillable),
        )
            .chain()
            .in_set(FixedGameSet::Act)
            .run_if(in_state(GamePhase::InGame))
            .run_if(simulation_is_running),
    );
}

fn adoption_options_are_fillable(
    vacancies: Query<
        (),
        (
            With<AdoptionOfferOption>,
            Without<CurrentAdoptionOffer>,
            Without<AdoptionOfferCooldown>,
        ),
    >,
) -> bool {
    !vacancies.is_empty()
}

fn initialize_adoption_offer_options(
    roots: Query<
        (Entity, &SelectedWorldIdentity, &Fame),
        (With<WorldRoot>, Without<AdoptionOfferRandom>),
    >,
    seed: Option<Res<ZooSeed>>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut commands: Commands,
) {
    let Some(seed) = seed else {
        return;
    };
    let Some(configuration) = definitions
        .get(&assets)
        .and_then(|view| view.animal_adoption_offer_configuration())
    else {
        return;
    };
    for (world, identity, fame) in &roots {
        let count = authored_adoption_slot_count(configuration, u16::from(fame.half_stars) * 10);
        if identity.mode == WorldSessionMode::Freeform {
            continue;
        }
        commands.entity(world).insert((
            AdoptionOfferClock(0.0),
            AdoptionOfferRandom(DeterministicRng::from_entity(
                *seed,
                PersistentId(0),
                RngDomain::AnimalAdoptionOffers,
            )),
            InitialAdoptionOfferPopulation,
        ));
        for slot in 0..count {
            for option in 0..2 {
                commands.spawn(AdoptionOfferOption {
                    world,
                    slot,
                    option,
                });
            }
        }
    }
}

fn resize_adoption_offer_options(
    roots: Query<(Entity, Option<&AdoptionOfferOptions>, &Fame), With<AdoptionOfferRandom>>,
    options: Query<(Entity, &AdoptionOfferOption, Option<&CurrentAdoptionOffer>)>,
    offers: Query<&AdoptionOfferSupply>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut commands: Commands,
) {
    let Some(configuration) = definitions
        .get(&assets)
        .and_then(|view| view.animal_adoption_offer_configuration())
    else {
        return;
    };
    for (world, members, fame) in &roots {
        let count = authored_adoption_slot_count(configuration, u16::from(fame.half_stars) * 10);
        let current_count = members
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .count()
            / 2;
        let current_count = u16::try_from(current_count).unwrap_or(u16::MAX);
        if current_count > count {
            for (entity, option, offer) in options
                .iter_many(members.into_iter().flat_map(RelationshipTarget::iter))
                .flatten()
            {
                if option.slot >= count {
                    if let Some(offer) = offer {
                        retire_adoption_offer(*offer.collection(), &offers, &mut commands);
                    }
                    commands.entity(entity).despawn();
                }
            }
        } else {
            for slot in current_count..count {
                for option in 0..2 {
                    commands.spawn(AdoptionOfferOption {
                        world,
                        slot,
                        option,
                    });
                }
            }
        }
    }
}

fn authored_adoption_slot_count(
    configuration: &AnimalAdoptionOfferConfiguration,
    fame: u16,
) -> u16 {
    configuration
        .base_slot_count
        .saturating_add(configuration.installed_expansion_slot_count)
        .saturating_add(
            configuration
                .fame_slots
                .iter()
                .filter(|threshold| threshold.fame_percent <= fame)
                .map(|threshold| threshold.additional_slot_count)
                .max()
                .unwrap_or_default(),
        )
}

fn advance_adoption_offer_deadlines(
    mut worlds: Query<&mut AdoptionOfferClock>,
    options: Query<(
        Entity,
        &AdoptionOfferOption,
        Option<&CurrentAdoptionOffer>,
        Option<&AdoptionOfferCooldown>,
    )>,
    offers: Query<&AdoptionOfferSupply>,
    waiting: Query<
        (Entity, &AdoptionOfferSupply, &WorldMember),
        (With<AdoptionOfferForOption>, Without<AdoptionOfferUnlocked>),
    >,
    time: Res<Time<Fixed>>,
    mut declines: MessageReader<DeclineCurrentAnimalAdoptionOffers>,
    mut commands: Commands,
) {
    let decline = declines.read().count() != 0;
    let delta = f64::from(time.delta_secs().max(0.0));
    for mut clock in &mut worlds {
        clock.0 += delta;
    }
    if !decline {
        for (entity, offer, member) in &waiting {
            if worlds.get(member.root).is_ok_and(|clock| {
                offer.unlock_at <= clock.0 && offer.remove_at.is_none_or(|due| due > clock.0)
            }) {
                commands.entity(entity).insert(AdoptionOfferUnlocked);
            }
        }
    }
    for (entity, option, offer, cooldown) in &options {
        let Ok(clock) = worlds.get(option.world) else {
            continue;
        };
        let Some((offer, supply)) = offer.and_then(|offer| {
            let entity = *offer.collection();
            offers.get(entity).ok().map(|supply| (entity, supply))
        }) else {
            if cooldown.is_some_and(|cooldown| decline || cooldown.0 <= clock.0) {
                commands.entity(entity).remove::<AdoptionOfferCooldown>();
            }
            continue;
        };
        if decline {
            let ready = clock.0 - delta + f64::from(supply.dismiss_cooldown);
            if ready > clock.0 {
                commands.entity(entity).insert(AdoptionOfferCooldown(ready));
            }
            retire_adoption_offer(offer, &offers, &mut commands);
        } else if supply.remove_at.is_some_and(|due| due <= clock.0) {
            retire_adoption_offer(offer, &offers, &mut commands);
        }
    }
}

fn retire_adoption_offer(
    offer: Entity,
    supplies: &Query<&AdoptionOfferSupply>,
    commands: &mut Commands,
) {
    if supplies
        .get(offer)
        .is_ok_and(AdoptionOfferSupply::has_pending_claims)
    {
        commands.entity(offer).remove::<AdoptionOfferForOption>();
    } else {
        commands.entity(offer).try_despawn();
    }
}

#[derive(Clone, Copy)]
struct AdoptionSpeciesOfferProfile {
    species: AssetId,
    minimum_fame: u16,
    sex_ranges: [[u16; 2]; 2],
    unlock_range: [f32; 2],
    remove_after: f32,
    dismiss_cooldown: f32,
}

fn adoption_offer_profiles(species_view: SpeciesView<'_>) -> Vec<AdoptionSpeciesOfferProfile> {
    species_view
        .species()
        .filter_map(|species| {
            if !species.flags.contains(SpeciesFlags::ADOPTABLE) {
                return None;
            }
            let mut ranges = [None; 2];
            let mut has_stock = false;
            for variant in species_view
                .variants(species.id)
                .filter(|variant| variant.life_stage == LifeStage::Adult)
            {
                let sex_index = match variant.sex {
                    Sex::Female => 0,
                    Sex::Male => 1,
                    Sex::Any => continue,
                };
                ranges[sex_index].get_or_insert(variant.adoption_count_range);
                has_stock |= variant.adoption_count_range[1] != 0;
            }
            let [Some(female), Some(male)] = ranges else {
                return None;
            };
            has_stock.then_some(AdoptionSpeciesOfferProfile {
                species: species.id,
                minimum_fame: species.adoption_offer.rarity_fame_percent,
                sex_ranges: [female, male],
                unlock_range: species.adoption_offer.unlock_seconds_range,
                remove_after: species.adoption_offer.remove_after_seconds,
                dismiss_cooldown: species.adoption_offer.dismiss_cooldown_seconds,
            })
        })
        .collect()
}

fn refill_adoption_offer_options<const INITIAL: bool>(
    mut worlds: Query<(
        Entity,
        &AdoptionOfferClock,
        &mut AdoptionOfferRandom,
        &SelectedWorldIdentity,
        Option<&AdoptionOfferOptions>,
        Has<InitialAdoptionOfferPopulation>,
        &Fame,
    )>,
    options: Query<(
        Entity,
        &AdoptionOfferOption,
        Option<&CurrentAdoptionOffer>,
        Has<AdoptionOfferCooldown>,
    )>,
    supplies: Query<&AdoptionOfferSupply>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    chance: Query<&SpeciesAdoptionChanceMultiplier>,
    mut profiles: Local<Option<Vec<AdoptionSpeciesOfferProfile>>>,
    mut commands: Commands,
) {
    let Some(species) = species_index.get(&species_assets) else {
        return;
    };
    if profiles.is_none() || species_assets.is_changed() || species_index.is_changed() {
        *profiles = Some(adoption_offer_profiles(species));
    }
    let Some(profiles) = profiles.as_deref() else {
        return;
    };
    let mut weights = None;
    for (world, clock, mut random, identity, members, initial, fame) in &mut worlds {
        if initial != INITIAL {
            continue;
        }
        let mut vacancies = Vec::new();
        let mut offered = HashSet::new();
        for (entity, option, offer, cooling) in options
            .iter_many(members.into_iter().flat_map(RelationshipTarget::iter))
            .flatten()
        {
            if let Some(offer) = offer {
                if let Ok(supply) = supplies.get(*offer.collection()) {
                    offered.insert(supply.species);
                }
            } else if !cooling {
                vacancies.push((option.slot, option.option, entity));
            }
        }
        if INITIAL {
            commands
                .entity(world)
                .remove::<InitialAdoptionOfferPopulation>();
        }
        if vacancies.is_empty() {
            continue;
        }
        vacancies.sort_unstable_by_key(|&(slot, option, _)| (slot, option));
        let threshold = if identity.mode == WorldSessionMode::Freeform {
            u16::MAX
        } else {
            u16::from(fame.half_stars) * 10
        };
        let weights = weights.get_or_insert_with(|| {
            let mut weights = HashMap::new();
            for multiplier in &chance {
                weights
                    .entry(multiplier.definition)
                    .or_insert(u32::from(multiplier.multiplier_permille));
            }
            weights
        });
        let mut candidates: Vec<_> = profiles
            .iter()
            .filter(|profile| {
                profile.minimum_fame <= threshold && !offered.contains(&profile.species)
            })
            .map(|profile| {
                (
                    profile,
                    weights.get(&profile.species).copied().unwrap_or(1000),
                )
            })
            .collect();
        for (_, _, entity) in vacancies {
            let total = candidates
                .iter()
                .map(|(_, weight)| *weight)
                .fold(0, u32::saturating_add);
            let Some(mut draw) = random.0.range_u32(total) else {
                break;
            };
            let Some(index) = candidates.iter().position(|(_, weight)| {
                if draw < *weight {
                    true
                } else {
                    draw -= *weight;
                    false
                }
            }) else {
                break;
            };
            let (profile, _) = candidates.swap_remove(index);
            let remaining = profile.sex_ranges.map(|[first, last]| {
                let width = u32::from(last.saturating_sub(first)) + 1;
                first.saturating_add(random.0.range_u32(width).unwrap_or_default() as u16)
            });
            if remaining == [0; 2] {
                continue;
            }
            let unlock_delay = if INITIAL {
                0.0
            } else {
                let [first, last] = profile.unlock_range;
                first + (last - first).max(0.0) * random.0.unit_f32()
            };
            let mut offer = commands.spawn((
                WorldMember { root: world },
                AdoptionOfferForOption(entity),
                AdoptionOfferSupply::new(
                    profile.species,
                    remaining,
                    clock.0 + f64::from(unlock_delay),
                    (profile.remove_after > 0.0)
                        .then_some(clock.0 + f64::from(profile.remove_after)),
                    profile.dismiss_cooldown,
                ),
            ));
            if unlock_delay <= 0.0 {
                offer.insert(AdoptionOfferUnlocked);
            }
        }
    }
}
