use bevy::prelude::*;
use openzt2_game_data::species::{LifeStage, SpeciesFlags};

use crate::assets::species::species_asset_types::SpeciesAsset;
use crate::assets::species::species_asset_types::SpeciesAssets;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::economy::account_transaction_types::Account;
use crate::plugins::economy::account_transaction_types::TransactionCompleted;
use crate::plugins::economy::account_transaction_types::TransactionKind;
use crate::plugins::economy::account_transaction_types::TransactionRejected;
use crate::plugins::economy::account_transaction_types::TransactionRequest;
use crate::plugins::habitat::habitat_types::Habitat;
use crate::plugins::progression::adoption_and_content_availability_types::AnimalAdoptionAvailabilityPolicy;
use crate::plugins::simulation_time::deterministic_random_stream::DeterministicRng;
use crate::plugins::simulation_time::deterministic_random_stream::RngDomain;
use crate::plugins::simulation_time::deterministic_random_stream::ZooSeed;
use crate::plugins::world_spawn::persistent_id_types::PersistentId;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    adoption_offer_inventory_types::AnimalAdoptionOfferInventory,
    animal_adoption_contracts::{AdoptAnimal, AnimalAdopted, PendingAdoption},
    animal_adoption_pricing::resolve_nonnegative_animal_adoption_cost,
    animal_habitat_placement_validation::{
        validate_animal_habitat_against_authored_aquatic_requirements, AnimalAquaticHabitatQueries,
    },
    animal_spawn_operation_contracts::{AnimalSpawnRejected, AnimalSpawned, SpawnAnimalRequest},
    animal_spawn_resolution::resolve_species_and_variant_to_animal_spawn_components,
    types::Parents,
};

/// Proof that the matching purchase completed before the natural spawn path
/// was requested. It lives only on the caller-owned operation entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CompletedAnimalAdoptionPayment;

pub(super) fn request_animal_adoption_payment_or_unpaid_spawn(
    mut commands: Commands,
    mut adoption_requests: MessageReader<AdoptAnimal>,
    zoo_seed: Res<ZooSeed>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    habitats: Query<&WorldMember, With<Habitat>>,
    adoption_policy: Query<&AnimalAdoptionAvailabilityPolicy, With<WorldRoot>>,
    adoption_offer_inventories: Query<&AnimalAdoptionOfferInventory, With<WorldRoot>>,
    aquatic_habitats: AnimalAquaticHabitatQueries,
    mut transaction_requests: MessageWriter<TransactionRequest>,
    mut spawn_requests: MessageWriter<SpawnAnimalRequest>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(species_index) = species_index.get(&species_assets) else {
        return;
    };
    for adoption_request in adoption_requests.read() {
        let Some(species) = species_index.find(adoption_request.species) else {
            continue;
        };
        if !species.flags.contains(SpeciesFlags::ADOPTABLE) {
            continue;
        }
        let Ok(world_membership) = habitats.get(adoption_request.habitat) else {
            continue;
        };

        let operation_entity = commands.spawn(*world_membership).id();
        let preview_identifier = PersistentId(operation_entity.to_bits().max(1));
        let mut random_stream =
            DeterministicRng::from_entity(*zoo_seed, preview_identifier, RngDomain::Reproduction);
        let Some(resolved_components) = resolve_species_and_variant_to_animal_spawn_components(
            species_index,
            species,
            adoption_request.variant,
            adoption_request.sex,
            LifeStage::Adult,
            world_definitions.timing().ticks_per_day,
            &mut random_stream,
        ) else {
            commands.entity(operation_entity).despawn();
            continue;
        };
        if let Some(offer_slot_index) = adoption_request.offer_slot_index.map(usize::from) {
            let offer_is_actionable = adoption_offer_inventories
                .get(world_membership.root)
                .ok()
                .is_some_and(|inventory| {
                    inventory.offer_is_actionable(
                        offer_slot_index,
                        adoption_request.species,
                        resolved_components.sex,
                    )
                });
            if !offer_is_actionable {
                commands.entity(operation_entity).despawn();
                continue;
            }
        }

        if validate_animal_habitat_against_authored_aquatic_requirements(
            world_definitions,
            adoption_request.species,
            adoption_request.habitat,
            &aquatic_habitats,
        )
        .is_err()
        {
            commands.entity(operation_entity).despawn();
            continue;
        }

        let Some(mut adoption_cost) =
            resolve_nonnegative_animal_adoption_cost(world_definitions, species)
        else {
            commands.entity(operation_entity).despawn();
            continue;
        };
        if let Ok(policy) = adoption_policy.get(world_membership.root) {
            if !policy.enabled {
                commands.entity(operation_entity).despawn();
                continue;
            }
            adoption_cost.0 = adoption_cost
                .0
                .saturating_mul(i64::from(policy.multiplier_permille))
                / 1000;
        }
        commands.entity(operation_entity).insert(PendingAdoption {
            species: adoption_request.species,
            variant: resolved_components.variant,
            sex: resolved_components.sex,
            offer_slot_index: adoption_request.offer_slot_index,
            habitat: adoption_request.habitat,
            transform: adoption_request.transform,
            cost: adoption_cost,
        });
        if adoption_cost.0 > 0 {
            transaction_requests.write(TransactionRequest {
                operation: operation_entity,
                debit: Account::Zoo,
                credit: Account::External,
                amount: adoption_cost,
                kind: TransactionKind::AnimalAdoption,
                subject: None,
            });
        } else {
            spawn_requests.write(SpawnAnimalRequest {
                operation: operation_entity,
                species: adoption_request.species,
                variant: Some(resolved_components.variant),
                sex: Some(resolved_components.sex),
                habitat: adoption_request.habitat,
                transform: adoption_request.transform,
                parents: Parents::NONE,
            });
        }
    }
}

pub(super) fn request_paid_animal_adoption_spawns_after_completed_transactions(
    mut commands: Commands,
    mut completed_transactions: MessageReader<TransactionCompleted>,
    pending_adoptions: Query<&PendingAdoption, Without<CompletedAnimalAdoptionPayment>>,
    mut spawn_requests: MessageWriter<SpawnAnimalRequest>,
) {
    for transaction in completed_transactions.read() {
        if transaction.kind != TransactionKind::AnimalAdoption {
            continue;
        }
        let Ok(adoption) = pending_adoptions.get(transaction.operation) else {
            continue;
        };
        commands
            .entity(transaction.operation)
            .insert(CompletedAnimalAdoptionPayment);
        spawn_requests.write(SpawnAnimalRequest {
            operation: transaction.operation,
            species: adoption.species,
            variant: Some(adoption.variant),
            sex: Some(adoption.sex),
            habitat: adoption.habitat,
            transform: adoption.transform,
            parents: Parents::NONE,
        });
    }
}

pub(super) fn reject_animal_adoptions_after_rejected_transactions(
    mut commands: Commands,
    mut rejected_transactions: MessageReader<TransactionRejected>,
    pending_adoptions: Query<(), With<PendingAdoption>>,
) {
    for transaction in rejected_transactions.read() {
        if transaction.kind == TransactionKind::AnimalAdoption
            && pending_adoptions.contains(transaction.operation)
        {
            commands.entity(transaction.operation).despawn();
        }
    }
}

pub(super) fn complete_animal_adoptions_after_successful_spawns(
    mut commands: Commands,
    mut spawned_animals: MessageReader<AnimalSpawned>,
    pending_adoptions: Query<&PendingAdoption>,
    mut adopted_animals: MessageWriter<AnimalAdopted>,
) {
    for spawned_animal in spawned_animals.read() {
        let Ok(adoption) = pending_adoptions.get(spawned_animal.operation) else {
            continue;
        };
        adopted_animals.write(AnimalAdopted {
            operation: spawned_animal.operation,
            animal: spawned_animal.animal,
            cost: adoption.cost,
            species: adoption.species,
            sex: adoption.sex,
            offer_slot_index: adoption.offer_slot_index,
        });
        commands.entity(spawned_animal.operation).despawn();
    }
}

pub(super) fn refund_paid_animal_adoptions_after_rejected_spawns(
    mut commands: Commands,
    mut rejected_spawns: MessageReader<AnimalSpawnRejected>,
    pending_adoptions: Query<(&PendingAdoption, Has<CompletedAnimalAdoptionPayment>)>,
    mut refund_requests: MessageWriter<TransactionRequest>,
) {
    for rejected_spawn in rejected_spawns.read() {
        let Ok((adoption, payment_completed)) = pending_adoptions.get(rejected_spawn.operation)
        else {
            continue;
        };
        if payment_completed {
            refund_requests.write(TransactionRequest {
                operation: rejected_spawn.operation,
                debit: Account::External,
                credit: Account::Zoo,
                amount: adoption.cost,
                kind: TransactionKind::Refund,
                subject: None,
            });
        }
        commands.entity(rejected_spawn.operation).despawn();
    }
}
