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
    AdoptionOfferCooldown, AdoptionOfferForOption, AdoptionOfferOption, AdoptionOfferOptions,
    AdoptionOfferRandom, AdoptionOfferSupply, AdoptionOfferTiming, AdoptionOfferUnlocked,
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
    worlds: Query<'w, 's, &'static AdoptionOfferOptions>,
    options: Query<'w, 's, (&'static AdoptionOfferOption, &'static CurrentAdoptionOffer)>,
    cooldowns: Query<'w, 's, &'static mut AdoptionOfferCooldown>,
    offers: Query<
        'w,
        's,
        (
            &'static mut AdoptionOfferSupply,
            Option<&'static AdoptionOfferForOption>,
            Has<AdoptionOfferUnlocked>,
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
        let options = self.worlds.get(world).ok()?;
        let offer = self
            .options
            .iter_many(options.iter())
            .flatten()
            .filter(|(option, _)| option.slot == slot)
            .find_map(|(_, current)| {
                let offer = *current.collection();
                let (supply, _, unlocked) = self.offers.get(offer).ok()?;
                (supply.species == species && unlocked && supply.available_by_sex[sex_index] != 0)
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
        let Ok((mut supply, option, _)) = self.offers.get_mut(offer) else {
            return;
        };
        supply.settle(sex_index, purchased);
        if supply.has_pending_claims() {
            return;
        }
        let Some(option_entity) = option
            .map(|option| option.0)
            .filter(|&entity| self.options.contains(entity))
        else {
            commands.entity(offer).try_despawn();
            return;
        };
        if supply.available_by_sex == [0; 2] {
            let cooldown =
                AdoptionOfferCooldown::from_remaining_seconds(f64::from(supply.dismiss_cooldown));
            if let Ok(mut current) = self.cooldowns.get_mut(option_entity) {
                *current = cooldown;
            } else {
                commands.entity(option_entity).insert(cooldown);
            }
            commands.entity(offer).try_despawn();
        }
    }
}

pub(super) fn register_adoption_offers(app: &mut App) {
    app.init_resource::<AdoptionSpeciesOfferProfiles>()
        .add_systems(
            Update,
            (
                initialize_adoption_offer_options,
                project_adoption_species_offer_profiles,
                refill_adoption_offer_options::<true>
                    .after(initialize_adoption_offer_options)
                    .after(project_adoption_species_offer_profiles),
            )
                .in_set(GameSet::Intent)
                .run_if(in_state(GamePhase::InGame)),
        )
        .add_systems(
            FixedUpdate,
            (
                resize_adoption_offer_options,
                decline_current_adoption_offers
                    .after(resize_adoption_offer_options)
                    .run_if(
                        bevy::ecs::schedule::common_conditions::on_message::<
                            DeclineCurrentAnimalAdoptionOffers,
                        >,
                    ),
                advance_adoption_offer_timers.after(decline_current_adoption_offers),
                advance_adoption_cooldowns.after(decline_current_adoption_offers),
                refill_adoption_offer_options::<false>
                    .after(advance_adoption_offer_timers)
                    .after(advance_adoption_cooldowns)
                    .run_if(adoption_options_are_fillable),
            )
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
    seed: Option<Single<&ZooSeed>>,
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
            AdoptionOfferRandom(DeterministicRng::from_entity(
                **seed,
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
    changed_roots: Query<
        (Entity, Option<&AdoptionOfferOptions>, &Fame),
        (
            With<AdoptionOfferRandom>,
            Or<(Changed<Fame>, Added<AdoptionOfferRandom>)>,
        ),
    >,
    options: Query<(Entity, &AdoptionOfferOption, Option<&CurrentAdoptionOffer>)>,
    offers: Query<&AdoptionOfferSupply>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    mut commands: Commands,
) {
    let modified = events.read().any(|event| {
        matches!(
            event,
            AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
        )
    });
    events.clear();
    let Some(configuration) = definitions
        .get(&assets)
        .and_then(|view| view.animal_adoption_offer_configuration())
    else {
        return;
    };
    let all = definitions.is_changed() || modified;
    let affected = all
        .then(|| roots.iter())
        .into_iter()
        .flatten()
        .chain((!all).then(|| changed_roots.iter()).into_iter().flatten());
    for (world, members, fame) in affected {
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

fn decline_current_adoption_offers(
    mut options: Query<
        (
            Entity,
            Option<&CurrentAdoptionOffer>,
            Option<&mut AdoptionOfferCooldown>,
        ),
        With<AdoptionOfferOption>,
    >,
    offers: Query<&AdoptionOfferSupply>,
    mut declines: MessageReader<DeclineCurrentAnimalAdoptionOffers>,
    mut commands: Commands,
) {
    declines.clear();
    for (entity, offer, cooling) in &mut options {
        if let Some((offer, supply)) = offer.and_then(|offer| {
            let entity = *offer.collection();
            offers.get(entity).ok().map(|supply| (entity, supply))
        }) {
            if supply.dismiss_cooldown > 0.0 {
                let cooldown = AdoptionOfferCooldown::from_remaining_seconds(f64::from(
                    supply.dismiss_cooldown,
                ));
                if let Some(mut current) = cooling {
                    *current = cooldown;
                } else {
                    commands.entity(entity).insert(cooldown);
                }
            }
            retire_adoption_offer(offer, &offers, &mut commands);
        } else if cooling.is_some() {
            commands.entity(entity).remove::<AdoptionOfferCooldown>();
        }
    }
}

fn advance_adoption_offer_timers(
    mut offers: Query<(Entity, &mut AdoptionOfferTiming), With<AdoptionOfferForOption>>,
    supplies: Query<&AdoptionOfferSupply>,
    time: Res<Time<Fixed>>,
    mut commands: Commands,
) {
    let delta = time.timestep();
    for (offer, mut timing) in &mut offers {
        if timing
            .retirement
            .as_mut()
            .is_some_and(|timer| timer.tick(delta).is_finished())
        {
            retire_adoption_offer(offer, &supplies, &mut commands);
            continue;
        }
        if timing
            .unlock
            .as_mut()
            .is_some_and(|timer| timer.tick(delta).is_finished())
        {
            commands.entity(offer).insert(AdoptionOfferUnlocked);
            timing.unlock = None;
        }
        if timing.unlock.is_none() && timing.retirement.is_none() {
            commands.entity(offer).remove::<AdoptionOfferTiming>();
        }
    }
}

fn advance_adoption_cooldowns(
    mut options: Query<(Entity, &mut AdoptionOfferCooldown), Without<CurrentAdoptionOffer>>,
    time: Res<Time<Fixed>>,
    mut commands: Commands,
) {
    let delta = time.timestep();
    for (option, mut cooldown) in &mut options {
        if cooldown.0.tick(delta).is_finished() {
            commands.entity(option).remove::<AdoptionOfferCooldown>();
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

#[derive(Clone, Copy, PartialEq)]
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

#[derive(Resource, Default, PartialEq)]
struct AdoptionSpeciesOfferProfiles(Option<Vec<AdoptionSpeciesOfferProfile>>);

fn project_adoption_species_offer_profiles(
    assets: Res<Assets<SpeciesAsset>>,
    index: Res<SpeciesAssets>,
    mut profiles: ResMut<AdoptionSpeciesOfferProfiles>,
) {
    if assets.is_changed() || index.is_changed() {
        profiles.set_if_neq(AdoptionSpeciesOfferProfiles(
            index.get(&assets).map(adoption_offer_profiles),
        ));
    }
}

fn refill_adoption_offer_options<const INITIAL: bool>(
    mut worlds: Query<(
        Entity,
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
    chance: Query<&SpeciesAdoptionChanceMultiplier>,
    profiles: Res<AdoptionSpeciesOfferProfiles>,
    mut commands: Commands,
) {
    let Some(profiles) = profiles.0.as_deref() else {
        return;
    };
    let mut weights = None;
    for (world, mut random, identity, members, initial, fame) in &mut worlds {
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
                AdoptionOfferSupply::new(profile.species, remaining, profile.dismiss_cooldown),
            ));
            if unlock_delay > 0.0 || profile.remove_after > 0.0 {
                offer.insert(AdoptionOfferTiming::from_remaining_seconds(
                    f64::from(unlock_delay),
                    (profile.remove_after > 0.0).then_some(f64::from(profile.remove_after)),
                ));
            }
            if unlock_delay <= 0.0 {
                offer.insert(AdoptionOfferUnlocked);
            }
        }
    }
}
