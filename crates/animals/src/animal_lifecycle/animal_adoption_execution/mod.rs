use bevy::prelude::*;
use openzt2_game_data::species::{LifeStage, SpeciesFlags};
use openzt2_zoo_state::placement::placement_surface_site::PlacementSiteQueries;

use openzt2_game_assets::species::species_asset_types::SpeciesAsset;
use openzt2_game_assets::species::species_asset_types::SpeciesAssets;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::economy::account_transaction_types::Account;
use openzt2_zoo_state::economy::account_transaction_types::TransactionCompleted;
use openzt2_zoo_state::economy::account_transaction_types::TransactionKind;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRejected;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRequest;
use openzt2_zoo_state::progression::adoption_and_content_availability_types::AnimalAdoptionAvailabilityPolicy;
use openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng;
use openzt2_world_state::simulation_time::deterministic_random_stream::RngDomain;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::{
    adoption_offer_execution::{AdoptionOfferClaim, AdoptionOfferClaims},
    animal_adoption_contracts::{
        AdoptAnimal, AnimalAdopted, AnimalAdoptionRefund, PendingAdoption,
    },
    animal_adoption_pricing::resolve_nonnegative_animal_adoption_cost,
    animal_spawn_operation_contracts::{AnimalSpawnRejected, AnimalSpawned, SpawnAnimalRequest},
    animal_spawn_resolution::select_species_spawn_variant,
    types::Parents,
};

/// Proof that the matching purchase completed before the natural spawn path
/// was requested. It lives only on the caller-owned operation entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CompletedAnimalAdoptionPayment;

pub(super) fn request_animal_adoption_payment_or_unpaid_spawn(
    mut commands: Commands,
    mut adoption_requests: MessageReader<AdoptAnimal>,
    species_assets: Res<Assets<SpeciesAsset>>,
    species_index: Res<SpeciesAssets>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    worlds: Query<(Entity, &ZooSeed), With<WorldRoot>>,
    adoption_policy: Query<&AnimalAdoptionAvailabilityPolicy, With<WorldRoot>>,
    mut offers: AdoptionOfferClaims,
    placement_sites: PlacementSiteQueries,
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
        let Ok((world, seed)) = worlds.get(adoption_request.world) else {
            continue;
        };

        let world_membership = WorldMember { root: world };
        let operation_entity = commands.spawn(world_membership).id();
        let preview_identifier = PersistentId(operation_entity.to_bits().max(1));
        let mut random_stream =
            DeterministicRng::from_entity(*seed, preview_identifier, RngDomain::Reproduction);
        let Some((variant, sex)) = select_species_spawn_variant(
            species_index,
            species,
            adoption_request.variant,
            adoption_request.sex,
            LifeStage::Adult,
            &mut random_stream,
        ) else {
            commands.entity(operation_entity).despawn();
            continue;
        };
        if placement_sites
            .validate(
                variant.placement_surface_policy,
                world,
                adoption_request.habitat,
                adoption_request.transform.translation,
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
        if let Some(slot) = adoption_request.offer_slot_index {
            let Some(claim) = offers.reserve(world, slot, adoption_request.species, sex) else {
                commands.entity(operation_entity).despawn();
                continue;
            };
            commands.entity(operation_entity).insert(claim);
        }
        commands.entity(operation_entity).insert(PendingAdoption {
            species: adoption_request.species,
            variant: variant.id,
            sex,
            offer_slot_index: adoption_request.offer_slot_index,
            world,
            habitat: adoption_request.habitat,
            transform: adoption_request.transform,
            cost: adoption_cost,
        });
        if adoption_cost.0 > 0 {
            transaction_requests.write(TransactionRequest {
                        domain: openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Player,
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
                variant: Some(variant.id),
                sex: Some(sex),
                world,
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
            world: adoption.world,
            transform: adoption.transform,
            parents: Parents::NONE,
        });
    }
}

pub(super) fn reject_animal_adoptions_after_rejected_transactions(
    mut commands: Commands,
    mut rejected_transactions: MessageReader<TransactionRejected>,
    mut pending_adoptions: Query<Option<&mut AdoptionOfferClaim>, With<PendingAdoption>>,
    mut offers: AdoptionOfferClaims,
) {
    for transaction in rejected_transactions.read() {
        if transaction.kind != TransactionKind::AnimalAdoption {
            continue;
        }
        let Ok(claim) = pending_adoptions.get_mut(transaction.operation) else {
            continue;
        };
        if let Some(mut claim) = claim {
            if claim.settled() {
                continue;
            }
            offers.settle(&mut claim, false, &mut commands);
        }
        commands.entity(transaction.operation).try_despawn();
    }
}

pub(super) fn complete_animal_adoptions_after_successful_spawns(
    mut commands: Commands,
    mut spawned_animals: MessageReader<AnimalSpawned>,
    mut pending_adoptions: Query<(&PendingAdoption, Option<&mut AdoptionOfferClaim>)>,
    mut offers: AdoptionOfferClaims,
    mut adopted_animals: MessageWriter<AnimalAdopted>,
) {
    for spawned_animal in spawned_animals.read() {
        let Ok((adoption, claim)) = pending_adoptions.get_mut(spawned_animal.operation) else {
            continue;
        };
        if let Some(mut claim) = claim {
            if claim.settled() {
                continue;
            }
            offers.settle(&mut claim, true, &mut commands);
        }
        adopted_animals.write(AnimalAdopted {
            operation: spawned_animal.operation,
            animal: spawned_animal.animal,
            cost: adoption.cost,
            species: adoption.species,
            sex: adoption.sex,
            offer_slot_index: adoption.offer_slot_index,
        });
        commands.entity(spawned_animal.operation).try_despawn();
    }
}

pub(super) fn refund_paid_animal_adoptions_after_rejected_spawns(
    mut commands: Commands,
    mut rejected_spawns: MessageReader<AnimalSpawnRejected>,
    mut pending_adoptions: Query<(
        &PendingAdoption,
        Has<CompletedAnimalAdoptionPayment>,
        Option<&mut AdoptionOfferClaim>,
    )>,
    mut offers: AdoptionOfferClaims,
    mut refund_requests: MessageWriter<TransactionRequest>,
) {
    for rejected_spawn in rejected_spawns.read() {
        let Ok((adoption, payment_completed, claim)) =
            pending_adoptions.get_mut(rejected_spawn.operation)
        else {
            continue;
        };
        if let Some(mut claim) = claim {
            if claim.settled() {
                continue;
            }
            offers.settle(&mut claim, false, &mut commands);
        }
        if payment_completed {
            let operation = commands
                .spawn((
                    AnimalAdoptionRefund,
                    WorldMember {
                        root: adoption.world,
                    },
                ))
                .id();
            refund_requests.write(TransactionRequest {
                        domain: openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Player,
                operation,
                debit: Account::External,
                credit: Account::Zoo,
                amount: adoption.cost,
                kind: TransactionKind::Refund,
                subject: None,
            });
        }
        commands.entity(rejected_spawn.operation).try_despawn();
    }
}

pub(super) fn remove_settled_animal_adoption_refunds(
    mut commands: Commands,
    refunds: Query<(), With<AnimalAdoptionRefund>>,
    mut completed: MessageReader<TransactionCompleted>,
    mut rejected: MessageReader<TransactionRejected>,
) {
    for operation in completed
        .read()
        .map(|transaction| transaction.operation)
        .chain(rejected.read().map(|transaction| transaction.operation))
    {
        if refunds.contains(operation) {
            commands.entity(operation).despawn();
        }
    }
}
