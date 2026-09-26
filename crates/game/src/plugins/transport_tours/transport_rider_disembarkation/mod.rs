use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::economy::account_transaction_types::Account;
use crate::plugins::economy::account_transaction_types::TransactionKind;
use crate::plugins::economy::account_transaction_types::TransactionRequest;
use crate::plugins::economy::facility_economy_types::Price;
use crate::plugins::guests::guest_simulation_types::Guest;
use crate::plugins::locomotion::locomotion_types::Arrived;
use crate::plugins::locomotion::locomotion_types::LocomotionMode;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    tour_scoring_types::{
        StationRatingPermille, TourObservationCadence, TourObservationMemory, TourScore,
    },
    tour_view_scoring::calculate_tour_rating_from_authored_score_ranges,
    transport_circuit_types::{CircuitMember, TransportCircuit},
    transport_fare_types::{PendingTransportFare, QueuedFare},
    transport_rider_types::{PlayerRidingSeatIndex, TransportRider},
    transport_topology_types::TransportStation,
    transport_vehicle_types::TransportVehicle,
};

pub(super) fn disembark_transport_riders_and_request_service_fare_transactions(
    mut vehicle_arrivals: MessageReader<Arrived>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut transport_vehicles: Query<(&mut TransportVehicle, &CircuitMember)>,
    transport_stations: Query<(&TransportStation, &CircuitMember)>,
    transport_riders: Query<(Entity, &TransportRider, &TourScore, &WorldMember), With<Guest>>,
    transport_circuits: Query<&Price, With<TransportCircuit>>,
    pending_transport_fares: Query<&PendingTransportFare>,
    mut transaction_requests: MessageWriter<TransactionRequest>,
    mut commands: Commands,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for vehicle_arrival in vehicle_arrivals.read() {
        let Some(station_entity) = vehicle_arrival.target else {
            continue;
        };
        let Ok((mut transport_vehicle, vehicle_circuit_member)) =
            transport_vehicles.get_mut(vehicle_arrival.entity)
        else {
            continue;
        };
        let Ok((_, station_circuit_member)) = transport_stations.get(station_entity) else {
            continue;
        };
        if station_circuit_member.0 != vehicle_circuit_member.0 {
            continue;
        }
        let Ok(admission_price) = transport_circuits.get(vehicle_circuit_member.0) else {
            continue;
        };
        for (guest_entity, transport_rider, tour_score, world_member) in &transport_riders {
            if transport_vehicle.occupied == 0 {
                break;
            }
            if transport_rider.vehicle != vehicle_arrival.entity
                || transport_rider.boarded_station == station_entity
                || pending_transport_fares
                    .iter()
                    .any(|pending_fare| pending_fare.guest == guest_entity)
            {
                continue;
            }
            transport_vehicle.occupied = transport_vehicle.occupied.saturating_sub(1);
            let tour_rating = calculate_tour_rating_from_authored_score_ranges(
                world_definitions,
                tour_score.value,
            );
            commands
                .entity(station_entity)
                .insert(StationRatingPermille(
                    (tour_rating.clamp(0.0, 1.0) * 1000.0).round() as u16,
                ));
            let fare_operation = commands
                .spawn((
                    *world_member,
                    PendingTransportFare {
                        guest: guest_entity,
                        circuit: vehicle_circuit_member.0,
                        score: tour_score.value,
                        rating: tour_rating,
                    },
                    QueuedFare::pending(admission_price.0),
                ))
                .id();
            transaction_requests.write(TransactionRequest {
                operation: fare_operation,
                debit: Account::Entity(guest_entity),
                credit: Account::Zoo,
                amount: admission_price.0,
                kind: TransactionKind::Service,
                subject: Some(vehicle_circuit_member.0),
            });
            commands
                .entity(guest_entity)
                .remove::<(
                    TransportRider,
                    TourScore,
                    TourObservationMemory,
                    TourObservationCadence,
                    PlayerRidingSeatIndex,
                )>()
                .insert(LocomotionMode::Ground);
        }
    }
}
