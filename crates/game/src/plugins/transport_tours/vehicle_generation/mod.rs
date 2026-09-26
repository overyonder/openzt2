use avian3d::prelude::RigidBody;
use bevy::{gltf::Gltf, prelude::*};

use crate::assets::scene_prefab::ScenePrefabAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::economy::account_transaction_types::Account;
use crate::plugins::economy::account_transaction_types::TransactionCompleted;
use crate::plugins::economy::account_transaction_types::TransactionKind;
use crate::plugins::economy::account_transaction_types::TransactionRejected;
use crate::plugins::economy::account_transaction_types::TransactionRequest;
use crate::plugins::economy::money_types::Money;
use crate::plugins::world_spawn::persistent_id_types::PersistentIdAllocator;
use crate::plugins::world_spawn::prefab_model_readiness::first_missing_prefab_collider_model_asset_id;
use crate::plugins::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    transport_circuit_types::{CircuitMember, CircuitVehicleDefinition, TransportCircuit},
    transport_vehicle_types::{
        GenerateTransportVehicleForCircuitRequest, PendingTransportVehiclePurchase,
    },
};

pub(super) fn request_payment_for_generated_transport_vehicles(
    mut vehicle_generation_requests: MessageReader<GenerateTransportVehicleForCircuitRequest>,
    circuits: Query<(&WorldMember, &CircuitVehicleDefinition), With<TransportCircuit>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut commands: Commands,
    mut transaction_requests: MessageWriter<TransactionRequest>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for request in vehicle_generation_requests.read() {
        if let Ok((world_member, selected_vehicle_definition)) = circuits.get(request.circuit) {
            if selected_vehicle_definition.0 != request.definition {
                continue;
            }
            let purchase_cost_cents = definitions
                .find_vehicle(request.definition)
                .map(|vehicle| vehicle.purchase_cost_cents);
            let Some(purchase_cost_cents) = purchase_cost_cents.filter(|cost| *cost > 0) else {
                continue;
            };
            let purchase_operation = commands
                .spawn((
                    PendingTransportVehiclePurchase {
                        circuit: request.circuit,
                        definition: request.definition,
                        cost: Money(purchase_cost_cents),
                        paid: false,
                    },
                    *world_member,
                ))
                .id();
            transaction_requests.write(TransactionRequest {
                operation: purchase_operation,
                debit: Account::Zoo,
                credit: Account::External,
                amount: Money(purchase_cost_cents),
                kind: TransactionKind::Purchase,
                subject: Some(request.circuit),
            });
        }
    }
}

pub(super) fn record_transport_vehicle_purchase_results(
    mut completed_transactions: MessageReader<TransactionCompleted>,
    mut rejected_transactions: MessageReader<TransactionRejected>,
    mut pending_purchases: Query<(&mut PendingTransportVehiclePurchase, &WorldMember)>,
    mut commands: Commands,
) {
    for completed_transaction in completed_transactions.read() {
        let Ok((mut pending_purchase, _world_member)) =
            pending_purchases.get_mut(completed_transaction.operation)
        else {
            continue;
        };
        if completed_transaction.debit != Account::Zoo
            || completed_transaction.credit != Account::External
            || completed_transaction.amount != pending_purchase.cost
            || completed_transaction.kind != TransactionKind::Purchase
            || completed_transaction.subject != Some(pending_purchase.circuit)
        {
            continue;
        }
        pending_purchase.paid = true;
    }
    for rejected_transaction in rejected_transactions.read() {
        let Ok((pending_purchase, _world_member)) =
            pending_purchases.get(rejected_transaction.operation)
        else {
            continue;
        };
        if rejected_transaction.debit != Account::Zoo
            || rejected_transaction.credit != Account::External
            || rejected_transaction.amount != pending_purchase.cost
            || rejected_transaction.kind != TransactionKind::Purchase
            || rejected_transaction.subject != Some(pending_purchase.circuit)
        {
            continue;
        }
        commands.entity(rejected_transaction.operation).despawn();
    }
}

/// Materializes paid vehicle purchases through the ordinary scene-prefab path.
///
/// A purchase remains live while its prefab or models load, so payment cannot
/// produce a definition-only placeholder without its authored presentation.
pub(super) fn spawn_paid_transport_vehicle_prefab_instances(
    pending_purchases: Query<(Entity, &PendingTransportVehiclePurchase, &WorldMember)>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    models: Res<Assets<Gltf>>,
    mut persistent_id_allocator: ResMut<PersistentIdAllocator>,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (purchase_operation, pending_purchase, world_member) in &pending_purchases {
        if !pending_purchase.paid {
            continue;
        }
        let Some((object_definition, scene_prefab)) = (|| {
            let vehicle = definitions.find_vehicle(pending_purchase.definition)?;
            let object_definition = vehicle.object;
            let object = definitions.find_object(object_definition)?;
            Some((object_definition, object.prefab))
        })() else {
            continue;
        };
        let Some(prefab_handle) = definitions.scene(scene_prefab) else {
            continue;
        };
        let Some(prefab) = prefabs.get(&prefab_handle) else {
            continue;
        };
        if first_missing_prefab_collider_model_asset_id(prefab, &models).is_some() {
            continue;
        }
        let Ok(persistent_id) = persistent_id_allocator.allocate(world_member.root) else {
            continue;
        };
        let vehicle_entity = spawn_loaded_scene_prefab_as_world_instance(
            &mut commands,
            prefab,
            prefab_handle,
            world_member.root,
            object_definition,
            persistent_id,
            Transform::IDENTITY,
            true,
            None,
            RigidBody::Dynamic,
        );
        commands
            .entity(vehicle_entity)
            .insert(CircuitMember(pending_purchase.circuit));
        commands.entity(purchase_operation).despawn();
    }
}
