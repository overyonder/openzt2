use avian3d::prelude::RigidBody;
use bevy::{
    asset::{AssetLoadFailedEvent, LoadState},
    ecs::system::SystemParam,
    gltf::Gltf,
    platform::collections::HashSet,
    prelude::*,
};

use openzt2_game_assets::{
    scene_prefab::ScenePrefabAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{scene_prefab::PrefabColliderSource, AssetId};
use openzt2_world::world_spawn::prefab_world_instance_spawning::spawn_loaded_scene_prefab_as_world_instance;
use openzt2_world_state::world_spawn::{
    persistent_id_types::{PersistentId, PersistentIdAllocator},
    world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    economy::{account_transaction_types::*, money_types::Money},
    transport_tours::{
        transport_circuit_types::{
            CircuitMember, CircuitMembers, CircuitVehicleDefinition, TransportCircuit,
        },
        transport_topology_types::TransportStation,
        transport_vehicle_types::GenerateTransportVehicleForCircuitRequest,
    },
};

#[derive(Component)]
#[relationship(relationship_target = VehiclePurchases)]
struct VehiclePurchaseFor(Entity);

#[derive(Component)]
#[relationship_target(relationship = VehiclePurchaseFor)]
struct VehiclePurchases(Vec<Entity>);

#[derive(Component)]
struct VehiclePurchase {
    object: AssetId,
    prefab: Handle<ScenePrefabAsset>,
    cost: Money,
}

#[derive(Component, PartialEq, Eq)]
struct ColliderModels(Box<[Handle<Gltf>]>);

#[derive(Component)]
struct VehicleAssetsReady;

#[derive(Component)]
struct RequestedVehiclePayment {
    circuit: Entity,
    station: Entity,
    id: PersistentId,
}

#[derive(Component)]
struct PaidVehiclePurchase;

#[derive(Component)]
struct VehiclePurchaseRefund;

#[derive(SystemParam)]
struct AssetState<'w> {
    server: Res<'w, AssetServer>,
    prefabs: Res<'w, Assets<ScenePrefabAsset>>,
    models: Res<'w, Assets<Gltf>>,
}

pub(super) fn start_transport_vehicle_purchases(
    mut requests: MessageReader<GenerateTransportVehicleForCircuitRequest>,
    circuits: Query<(
        &WorldMember,
        &TransportCircuit,
        &CircuitVehicleDefinition,
        &CircuitMembers,
    )>,
    stations: Query<&TransportStation>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for request in requests.read() {
        let Ok((world, circuit, selected, members)) = circuits.get(request.circuit) else {
            continue;
        };
        let Some(vehicle) = definitions.find_vehicle(request.definition) else {
            continue;
        };
        let same_kind = definitions
            .find_station(circuit.definition)
            .map(|station| station.kind)
            .or_else(|| {
                definitions
                    .find_track(circuit.definition)
                    .map(|track| track.kind)
            })
            == Some(vehicle.kind);
        if selected.0 != vehicle.id
            || !same_kind
            || vehicle.purchase_cost_cents <= 0
            || !members.iter().any(|entity| stations.contains(entity))
        {
            continue;
        }
        let Some(prefab) = definitions
            .find_object(vehicle.object)
            .and_then(|object| definitions.scene(object.prefab))
        else {
            continue;
        };
        commands.spawn((
            VehiclePurchaseFor(request.circuit),
            VehiclePurchase {
                object: vehicle.object,
                prefab,
                cost: Money::from_cents(vehicle.purchase_cost_cents),
            },
            *world,
        ));
    }
}

fn observe_purchase_assets(
    mut commands: Commands,
    assets: AssetState,
    mut prefab_events: MessageReader<AssetEvent<ScenePrefabAsset>>,
    mut prefab_failures: MessageReader<AssetLoadFailedEvent<ScenePrefabAsset>>,
    mut model_events: MessageReader<AssetEvent<Gltf>>,
    mut model_failures: MessageReader<AssetLoadFailedEvent<Gltf>>,
    new_purchases: Query<Entity, Added<VehiclePurchase>>,
    mut purchases: Query<
        (Entity, Ref<VehiclePurchase>, Option<&mut ColliderModels>),
        (
            Without<VehicleAssetsReady>,
            Without<RequestedVehiclePayment>,
        ),
    >,
) {
    let changed_prefabs: HashSet<_> = prefab_events
        .read()
        .filter_map(available_asset)
        .chain(prefab_failures.read().map(|failure| failure.id))
        .collect();
    let changed_models: HashSet<_> = model_events
        .read()
        .filter_map(available_asset)
        .chain(model_failures.read().map(|failure| failure.id))
        .collect();
    if changed_prefabs.is_empty() && changed_models.is_empty() && new_purchases.is_empty() {
        return;
    }
    for (operation, purchase, retained_models) in &mut purchases {
        if !purchase.is_added()
            && !changed_prefabs.contains(&purchase.prefab.id())
            && !retained_models.as_ref().is_some_and(|retained| {
                retained
                    .0
                    .iter()
                    .any(|model| changed_models.contains(&model.id()))
            })
        {
            continue;
        }
        if matches!(
            assets.server.get_load_state(purchase.prefab.id()),
            Some(LoadState::Failed(_))
        ) {
            commands.entity(operation).try_despawn();
            continue;
        }
        let Some(prefab) = assets.prefabs.get(&purchase.prefab) else {
            continue;
        };
        let Some(models) = prefab
            .canonical_scene_prefab_document()
            .entities
            .iter()
            .flat_map(|entity| &entity.colliders)
            .filter_map(|collider| match &collider.source {
                PrefabColliderSource::Model { model, .. } => Some(AssetId(model.0)),
                _ => None,
            })
            .map(|model| prefab.loaded_model_asset_handle(model).cloned())
            .collect::<Option<Box<[_]>>>()
        else {
            commands.entity(operation).try_despawn();
            continue;
        };
        if models.iter().any(|model| {
            matches!(
                assets.server.get_load_state(model.id()),
                Some(LoadState::Failed(_))
            )
        }) {
            commands.entity(operation).try_despawn();
        } else {
            let ready = models
                .iter()
                .all(|model| assets.models.contains(model.id()));
            if let Some(mut retained) = retained_models {
                retained.set_if_neq(ColliderModels(models));
            } else {
                commands.entity(operation).insert(ColliderModels(models));
            }
            if ready {
                commands.entity(operation).insert(VehicleAssetsReady);
            }
        }
    }
}

fn available_asset<A: Asset>(event: &AssetEvent<A>) -> Option<bevy::asset::AssetId<A>> {
    match event {
        AssetEvent::Added { id }
        | AssetEvent::Modified { id }
        | AssetEvent::LoadedWithDependencies { id } => Some(*id),
        _ => None,
    }
}

fn request_payment_for_ready_vehicles(
    purchases: Query<
        (Entity, &VehiclePurchaseFor, &VehiclePurchase, &WorldMember),
        Added<VehicleAssetsReady>,
    >,
    circuits: Query<&CircuitMembers, With<TransportCircuit>>,
    stations: Query<&GlobalTransform, With<TransportStation>>,
    mut ids: Query<&mut PersistentIdAllocator>,
    mut requests: MessageWriter<TransactionRequest>,
    mut commands: Commands,
) {
    for (operation, owner, purchase, world) in &purchases {
        let station = circuits
            .get(owner.0)
            .ok()
            .and_then(|members| members.iter().find(|&member| stations.contains(member)));
        let Some((station, id)) = station
            .and_then(|station| Some((station, ids.get_mut(world.root).ok()?.allocate().ok()?)))
        else {
            commands.entity(operation).try_despawn();
            continue;
        };
        commands.entity(operation).insert(RequestedVehiclePayment {
            circuit: owner.0,
            station,
            id,
        });
        requests.write(TransactionRequest {
            domain:
                openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Player,
            operation,
            debit: Account::Zoo,
            credit: Account::External,
            amount: purchase.cost,
            kind: TransactionKind::Purchase,
            subject: Some(owner.0),
        });
    }
}

fn cancel_removed_circuit_purchases(
    removed: On<Remove<TransportCircuit>>,
    circuits: Query<&VehiclePurchases>,
    paying: Query<(), With<RequestedVehiclePayment>>,
    mut commands: Commands,
) {
    if let Ok(purchases) = circuits.get(removed.entity) {
        for operation in purchases.iter() {
            if !paying.contains(operation) {
                commands.entity(operation).try_despawn();
            }
        }
    }
}

fn record_vehicle_purchase_payments(
    mut completed: MessageReader<TransactionCompleted>,
    mut rejected: MessageReader<TransactionRejected>,
    purchases: Query<(&VehiclePurchase, &RequestedVehiclePayment), Without<PaidVehiclePurchase>>,
    refunds: Query<(), With<VehiclePurchaseRefund>>,
    mut commands: Commands,
) {
    for operation in completed.read() {
        if refunds.contains(operation.operation) {
            commands.entity(operation.operation).try_despawn();
            continue;
        }
        let Ok((purchase, payment)) = purchases.get(operation.operation) else {
            continue;
        };
        if (
            operation.debit,
            operation.credit,
            operation.amount,
            operation.kind,
            operation.subject,
        ) == (
            Account::Zoo,
            Account::External,
            purchase.cost,
            TransactionKind::Purchase,
            Some(payment.circuit),
        ) {
            commands
                .entity(operation.operation)
                .insert(PaidVehiclePurchase);
        }
    }
    for operation in rejected.read() {
        if refunds.contains(operation.operation) {
            warn!(reason = ?operation.reason, "vehicle purchase refund rejected");
            commands.entity(operation.operation).try_despawn();
            continue;
        }
        let Ok((purchase, payment)) = purchases.get(operation.operation) else {
            continue;
        };
        if (
            operation.debit,
            operation.credit,
            operation.amount,
            operation.kind,
            operation.subject,
        ) == (
            Account::Zoo,
            Account::External,
            purchase.cost,
            TransactionKind::Purchase,
            Some(payment.circuit),
        ) {
            commands.entity(operation.operation).try_despawn();
        }
    }
}

fn spawn_paid_transport_vehicle_prefab_instances(
    purchases: Query<
        (
            Entity,
            &VehiclePurchase,
            &RequestedVehiclePayment,
            &WorldMember,
            &ColliderModels,
        ),
        With<PaidVehiclePurchase>,
    >,
    assets: AssetState,
    circuits: Query<(), With<TransportCircuit>>,
    stations: Query<(&GlobalTransform, &CircuitMember, &WorldMember), With<TransportStation>>,
    mut commands: Commands,
    mut requests: MessageWriter<TransactionRequest>,
) {
    for (operation, purchase, payment, world, models) in &purchases {
        let pose = stations
            .get(payment.station)
            .ok()
            .filter(|(_, member, station_world)| {
                member.0 == payment.circuit && world.root == station_world.root
            })
            .map(|(pose, _, _)| pose.compute_transform());
        let prefab = assets.prefabs.get(&purchase.prefab);
        if let (true, Some(pose), Some(prefab)) = (
            circuits.contains(payment.circuit)
                && models
                    .0
                    .iter()
                    .all(|model| assets.models.contains(model.id())),
            pose,
            prefab,
        ) {
            let vehicle = spawn_loaded_scene_prefab_as_world_instance(
                &mut commands,
                prefab,
                purchase.prefab.clone(),
                world.root,
                purchase.object,
                payment.id,
                pose,
                true,
                None,
                RigidBody::Dynamic,
            );
            commands
                .entity(vehicle)
                .insert(CircuitMember(payment.circuit));
        } else {
            let refund = commands.spawn((*world, VehiclePurchaseRefund)).id();
            requests.write(TransactionRequest {
                domain: openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Player,
                operation: refund,
                debit: Account::External,
                credit: Account::Zoo,
                amount: purchase.cost,
                kind: TransactionKind::Refund,
                subject: None,
            });
        }
        commands.entity(operation).try_despawn();
    }
}

pub(super) fn install(app: &mut App) {
    app.add_observer(cancel_removed_circuit_purchases)
        .add_systems(
            Update,
            (
                observe_purchase_assets.after(start_transport_vehicle_purchases),
                request_payment_for_ready_vehicles.after(observe_purchase_assets),
                record_vehicle_purchase_payments,
                spawn_paid_transport_vehicle_prefab_instances
                    .after(record_vehicle_purchase_payments),
            )
                .run_if(in_state(
                    openzt2_game_assets::application_lifecycle::GamePhase::InGame,
                )),
        );
}
