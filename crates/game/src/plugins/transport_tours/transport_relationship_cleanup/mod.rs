use bevy::prelude::*;

use crate::plugins::{
    guests::guest_simulation_types::Guest, locomotion::locomotion_types::LocomotionMode,
};

use super::{
    tour_scoring_types::{TourObservationCadence, TourObservationMemory, TourScore},
    transport_circuit_types::{CircuitMember, TransportCircuit},
    transport_rider_types::{PlayerRidingSeatIndex, TransportRider, WaitingForTransport},
    transport_topology_types::{TrackSegment, TransportStation},
    transport_vehicle_types::{RoutePosition, TransportVehicle},
};

fn return_transport_rider_to_ground_locomotion(rider_entity: Entity, commands: &mut Commands) {
    commands
        .entity(rider_entity)
        .remove::<(
            TransportRider,
            TourScore,
            TourObservationMemory,
            TourObservationCadence,
            PlayerRidingSeatIndex,
        )>()
        .insert(LocomotionMode::Ground);
}

pub(super) fn remove_transport_relationships_broken_by_removed_circuits_stations_tracks_or_vehicles(
    mut removed_transport_circuits: RemovedComponents<TransportCircuit>,
    mut removed_transport_stations: RemovedComponents<TransportStation>,
    mut removed_track_segments: RemovedComponents<TrackSegment>,
    mut removed_transport_vehicles: RemovedComponents<TransportVehicle>,
    circuit_members: Query<(Entity, &CircuitMember)>,
    waiting_guests: Query<(Entity, &WaitingForTransport), With<Guest>>,
    transport_riders: Query<(Entity, &TransportRider), With<Guest>>,
    vehicle_routes: Query<(Entity, &RoutePosition)>,
    mut transport_stations: Query<&mut TransportStation>,
    mut transport_vehicles: Query<&mut TransportVehicle>,
    mut transport_circuits: Query<&mut TransportCircuit>,
    mut commands: Commands,
) {
    for removed_circuit_entity in removed_transport_circuits.read() {
        for (member_entity, circuit_member) in &circuit_members {
            if circuit_member.0 == removed_circuit_entity {
                commands.entity(member_entity).remove::<CircuitMember>();
            }
        }
        for (guest_entity, waiting_relation) in &waiting_guests {
            if waiting_relation.circuit == removed_circuit_entity {
                if let Ok(mut transport_station) =
                    transport_stations.get_mut(waiting_relation.station)
                {
                    transport_station.queued = transport_station.queued.saturating_sub(1);
                }
                commands
                    .entity(guest_entity)
                    .remove::<WaitingForTransport>();
            }
        }
        for (rider_entity, transport_rider) in &transport_riders {
            if circuit_members
                .get(transport_rider.vehicle)
                .is_ok_and(|(_, circuit_member)| circuit_member.0 == removed_circuit_entity)
            {
                return_transport_rider_to_ground_locomotion(rider_entity, &mut commands);
                if let Ok(mut transport_vehicle) =
                    transport_vehicles.get_mut(transport_rider.vehicle)
                {
                    transport_vehicle.occupied = transport_vehicle.occupied.saturating_sub(1);
                }
            }
        }
    }

    for removed_station_entity in removed_transport_stations.read() {
        if let Ok((_, circuit_member)) = circuit_members.get(removed_station_entity) {
            if let Ok(mut transport_circuit) = transport_circuits.get_mut(circuit_member.0) {
                transport_circuit.closed = true;
            }
        }
        for (guest_entity, waiting_relation) in &waiting_guests {
            if waiting_relation.station == removed_station_entity {
                commands
                    .entity(guest_entity)
                    .remove::<WaitingForTransport>();
            }
        }
        for (rider_entity, transport_rider) in &transport_riders {
            if transport_rider.boarded_station == removed_station_entity {
                return_transport_rider_to_ground_locomotion(rider_entity, &mut commands);
                if let Ok(mut transport_vehicle) =
                    transport_vehicles.get_mut(transport_rider.vehicle)
                {
                    transport_vehicle.occupied = transport_vehicle.occupied.saturating_sub(1);
                }
            }
        }
    }

    for removed_track_segment_entity in removed_track_segments.read() {
        if let Ok((_, circuit_member)) = circuit_members.get(removed_track_segment_entity) {
            if let Ok(mut transport_circuit) = transport_circuits.get_mut(circuit_member.0) {
                transport_circuit.closed = true;
            }
        }
        for (vehicle_entity, route_position) in &vehicle_routes {
            if route_position.segment != removed_track_segment_entity {
                continue;
            }
            commands.entity(vehicle_entity).remove::<RoutePosition>();
            for (rider_entity, transport_rider) in &transport_riders {
                if transport_rider.vehicle == vehicle_entity {
                    return_transport_rider_to_ground_locomotion(rider_entity, &mut commands);
                }
            }
            if let Ok(mut transport_vehicle) = transport_vehicles.get_mut(vehicle_entity) {
                transport_vehicle.occupied = 0;
            }
            if let Ok((_, circuit_member)) = circuit_members.get(vehicle_entity) {
                if let Ok(mut transport_circuit) = transport_circuits.get_mut(circuit_member.0) {
                    transport_circuit.closed = true;
                }
            }
        }
    }

    for removed_vehicle_entity in removed_transport_vehicles.read() {
        for (rider_entity, transport_rider) in &transport_riders {
            if transport_rider.vehicle == removed_vehicle_entity {
                return_transport_rider_to_ground_locomotion(rider_entity, &mut commands);
            }
        }
        if let Ok((_, circuit_member)) = circuit_members.get(removed_vehicle_entity) {
            if let Ok(mut transport_circuit) = transport_circuits.get_mut(circuit_member.0) {
                transport_circuit.closed = true;
            }
        }
    }
}
