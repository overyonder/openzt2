use bevy::prelude::*;
use std::collections::HashSet;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::economy::facility_economy_types::Price;
use crate::plugins::economy::facility_economy_types::Wallet;
use crate::plugins::guests::guest_simulation_types::Guest;
use crate::plugins::immersive_modes::immersive_mode_state_types::ControlledEntity;
use crate::plugins::locomotion::locomotion_types::Arrived;
use crate::plugins::locomotion::locomotion_types::Destination;
use crate::plugins::locomotion::locomotion_types::Docking;
use crate::plugins::locomotion::locomotion_types::LocomotionMode;
use crate::plugins::locomotion::locomotion_types::NavAgent;
use crate::plugins::locomotion::locomotion_types::Route;
use crate::plugins::locomotion::locomotion_types::Steering;

use super::{
    tour_scoring_types::{TourObservationCadence, TourObservationMemory, TourScore},
    transport_circuit_types::{CircuitMember, TransportCircuit},
    transport_rider_types::{
        BoardTransportRequest, PlayerRidingSeatIndex, TransportRider, WaitingForTransport,
    },
    transport_topology_types::TransportStation,
    transport_vehicle_types::TransportVehicle,
};

pub(super) fn accept_affordable_transport_boarding_requests_at_reachable_stations(
    mut boarding_requests: MessageReader<BoardTransportRequest>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    guests: Query<(
        &Guest,
        &Wallet,
        Option<&WaitingForTransport>,
        Option<&TransportRider>,
        &GlobalTransform,
        &NavAgent,
    )>,
    mut transport_stations: Query<(&CircuitMember, &mut TransportStation, &GlobalTransform)>,
    transport_circuits: Query<(&TransportCircuit, Option<&Price>)>,
    mut commands: Commands,
    mut guests_accepted_during_current_tick: Local<Vec<Entity>>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    guests_accepted_during_current_tick.clear();
    for boarding_request in boarding_requests.read() {
        if guests_accepted_during_current_tick.contains(&boarding_request.guest) {
            continue;
        }
        let Ok((_, wallet, waiting_relation, rider_relation, guest_transform, nav_agent)) =
            guests.get(boarding_request.guest)
        else {
            continue;
        };
        if waiting_relation.is_some() || rider_relation.is_some() {
            continue;
        }
        let Ok((circuit_member, mut transport_station, station_transform)) =
            transport_stations.get_mut(boarding_request.station)
        else {
            continue;
        };
        let Ok((transport_circuit, admission_price)) = transport_circuits.get(circuit_member.0)
        else {
            continue;
        };
        let Some(authored_station) = world_definitions.find_station(transport_station.definition)
        else {
            continue;
        };
        let guest_can_reach_dock = authored_station
            .passenger_entry_offsets_cm
            .into_iter()
            .map(|[x, z]| Vec3::new(f32::from(x), 0.0, f32::from(z)) * 0.01)
            .map(|offset| station_transform.transform_point(offset))
            .any(|dock_position| {
                guest_transform
                    .translation()
                    .distance_squared(dock_position)
                    <= nav_agent.radius_m * nav_agent.radius_m
            });
        // The economy system rejects non-positive transactions, so only circuits with an
        // explicit payable fare may admit riders into the paid trip lifecycle.
        let guest_can_afford_fare =
            admission_price.is_some_and(|price| price.0.is_positive() && wallet.0 >= price.0);
        if transport_circuit.closed
            || !guest_can_reach_dock
            || !guest_can_afford_fare
            || transport_station
                .queued
                .saturating_add(transport_station.occupied)
                >= transport_station.capacity
        {
            continue;
        }
        transport_station.queued += 1;
        guests_accepted_during_current_tick.push(boarding_request.guest);
        commands
            .entity(boarding_request.guest)
            .insert(WaitingForTransport {
                station: boarding_request.station,
                circuit: circuit_member.0,
            });
    }
}

fn select_next_waiting_guest_by_stable_entity_order(
    station_entity: Entity,
    transport_circuit_entity: Entity,
    previous_guest_entity: Option<Entity>,
    boarded_during_current_tick: &HashSet<Entity>,
    waiting_guests: &Query<(Entity, &WaitingForTransport), With<Guest>>,
) -> Option<Entity> {
    waiting_guests
        .iter()
        .filter(|(guest_entity, waiting_relation)| {
            !boarded_during_current_tick.contains(guest_entity)
                && waiting_relation.station == station_entity
                && waiting_relation.circuit == transport_circuit_entity
                && previous_guest_entity
                    .is_none_or(|previous| guest_entity.to_bits() > previous.to_bits())
        })
        .min_by_key(|(guest_entity, _)| guest_entity.to_bits())
        .map(|(guest_entity, _)| guest_entity)
}

pub(super) fn board_waiting_guests_when_transport_vehicle_arrives(
    mut vehicle_arrivals: MessageReader<Arrived>,
    mut transport_vehicles: Query<(Entity, &mut TransportVehicle, &CircuitMember)>,
    mut transport_stations: Query<(Entity, &CircuitMember, &mut TransportStation)>,
    waiting_guests: Query<(Entity, &WaitingForTransport), With<Guest>>,
    locally_controlled_guests: Query<(), With<ControlledEntity>>,
    mut boarded_during_current_tick: Local<HashSet<Entity>>,
    mut commands: Commands,
) {
    boarded_during_current_tick.clear();
    for vehicle_arrival in vehicle_arrivals.read() {
        let Some(station_entity) = vehicle_arrival.target else {
            continue;
        };
        let Ok((vehicle_entity, mut transport_vehicle, vehicle_circuit_member)) =
            transport_vehicles.get_mut(vehicle_arrival.entity)
        else {
            continue;
        };
        let Ok((_, station_circuit_member, mut transport_station)) =
            transport_stations.get_mut(station_entity)
        else {
            continue;
        };
        if station_circuit_member.0 != vehicle_circuit_member.0 {
            continue;
        }
        let boarding_capacity = transport_station.queued.min(
            transport_vehicle
                .seats
                .saturating_sub(transport_vehicle.occupied),
        );
        let mut previous_guest_entity = None;
        for _ in 0..boarding_capacity {
            let Some(guest_entity) = select_next_waiting_guest_by_stable_entity_order(
                station_entity,
                vehicle_circuit_member.0,
                previous_guest_entity,
                &boarded_during_current_tick,
                &waiting_guests,
            ) else {
                break;
            };
            previous_guest_entity = Some(guest_entity);
            boarded_during_current_tick.insert(guest_entity);
            transport_station.queued = transport_station.queued.saturating_sub(1);
            let assigned_seat_index = transport_vehicle.occupied;
            transport_vehicle.occupied += 1;
            let mut guest_commands = commands.entity(guest_entity);
            guest_commands
                .remove::<(WaitingForTransport, Destination, Route, Docking, Steering)>()
                .insert((
                    TransportRider {
                        vehicle: vehicle_entity,
                        boarded_station: station_entity,
                        seat_index: assigned_seat_index,
                    },
                    TourScore::default(),
                    TourObservationMemory::default(),
                    TourObservationCadence::default(),
                    LocomotionMode::Vehicle,
                ));
            if locally_controlled_guests.get(guest_entity).is_ok() {
                guest_commands.insert(PlayerRidingSeatIndex(assigned_seat_index));
            }
        }
    }
}
