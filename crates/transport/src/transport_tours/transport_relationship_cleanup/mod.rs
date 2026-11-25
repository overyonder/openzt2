use bevy::prelude::*;

use openzt2_zoo_state::{
    locomotion::locomotion_types::LocomotionMode,
    transport_tours::{
        tour_scoring_types::{TourObservationMemory, TourScore},
        transport_circuit_types::{CircuitMember, CircuitMembers, TransportCircuit},
        transport_rider_types::*,
        transport_topology_types::{TrackEnd, TrackSegment, TrackStart, TransportStation},
        transport_vehicle_types::{
            RoutePosition, TransportVehicle, VehicleOnTrack, VehiclesOnTrack,
        },
    },
};

pub(super) fn install(app: &mut App) {
    app.add_observer(remove_circuit_membership)
        .add_observer(detach_removed_circuit_member)
        .add_observer(detach_removed_station)
        .add_observer(detach_removed_track)
        .add_observer(close_disconnected_track)
        .add_observer(detach_removed_vehicle)
        .add_observer(remove_retired_seats)
        .add_observer(unseat_riders)
        .add_observer(detach_unseated_rider)
        .add_observer(return_rider_to_ground);
}

fn remove_circuit_membership(
    removed: On<Remove<TransportCircuit>>,
    circuits: Query<&CircuitMembers>,
    mut commands: Commands,
) {
    if let Ok(members) = circuits.get(removed.entity) {
        for member in members.iter() {
            commands.entity(member).try_remove::<CircuitMember>();
        }
    }
}

fn detach_removed_circuit_member(
    removed: On<Discard<CircuitMember>>,
    members: Query<&CircuitMember>,
    stations: Query<(Option<&StationQueue>, Option<&StationBoardedRiders>), With<TransportStation>>,
    vehicles: Query<&VehicleRiders>,
    mut circuits: Query<&mut TransportCircuit>,
    mut commands: Commands,
) {
    close_owner(removed.entity, &members, &mut circuits);
    if let Ok((queue, boarded)) = stations.get(removed.entity) {
        detach_station_riders(queue, boarded, &mut commands);
    }
    if let Ok(riders) = vehicles.get(removed.entity) {
        dismount(riders.iter(), &mut commands);
    }
}

fn detach_removed_station(
    removed: On<Remove<TransportStation>>,
    stations: Query<(Option<&StationQueue>, Option<&StationBoardedRiders>)>,
    members: Query<&CircuitMember>,
    mut circuits: Query<&mut TransportCircuit>,
    mut commands: Commands,
) {
    close_owner(removed.entity, &members, &mut circuits);
    if let Ok((queue, boarded)) = stations.get(removed.entity) {
        detach_station_riders(queue, boarded, &mut commands);
    }
}

fn detach_station_riders(
    queue: Option<&StationQueue>,
    boarded: Option<&StationBoardedRiders>,
    commands: &mut Commands,
) {
    if let Some(waiting) = queue {
        for guest in waiting.iter() {
            commands.entity(guest).try_remove::<WaitingForTransport>();
        }
    }
    if let Some(boarded) = boarded {
        dismount(boarded.iter(), commands);
    }
}

fn detach_removed_vehicle(
    removed: On<Remove<TransportVehicle>>,
    vehicles: Query<&VehicleRiders>,
    members: Query<&CircuitMember>,
    mut circuits: Query<&mut TransportCircuit>,
    mut commands: Commands,
) {
    close_owner(removed.entity, &members, &mut circuits);
    if let Ok(riders) = vehicles.get(removed.entity) {
        dismount(riders.iter(), &mut commands);
    }
}

fn remove_retired_seats(
    removed: On<Remove<TransportVehicle>>,
    vehicles: Query<&Children>,
    seats: Query<(), With<TransportSeat>>,
    mut commands: Commands,
) {
    if let Ok(children) = vehicles.get(removed.entity) {
        for seat in children.iter().filter(|&child| seats.contains(child)) {
            commands.entity(seat).try_despawn();
        }
    }
    commands
        .entity(removed.entity)
        .try_remove::<super::transport_passenger_mounts::VehicleSeatsHydrated>();
}

fn unseat_riders(
    removed: On<Remove<TransportSeat>>,
    seats: Query<&RidersAtSeat>,
    mut commands: Commands,
) {
    if let Ok(riders) = seats.get(removed.entity) {
        dismount(riders.iter(), &mut commands);
    }
}

fn detach_unseated_rider(removed: On<Remove<TransportSeatRider>>, mut commands: Commands) {
    commands
        .entity(removed.entity)
        .try_remove::<TransportRider>();
}

fn detach_removed_track(
    removed: On<Remove<TrackSegment>>,
    vehicles_on_track: Query<&VehiclesOnTrack>,
    riders: Query<&VehicleRiders>,
    members: Query<&CircuitMember>,
    mut circuits: Query<&mut TransportCircuit>,
    mut commands: Commands,
) {
    close_owner(removed.entity, &members, &mut circuits);
    if let Ok(vehicles) = vehicles_on_track.get(removed.entity) {
        for vehicle in vehicles.iter() {
            commands.entity(vehicle).try_remove::<(
                RoutePosition,
                VehicleOnTrack,
                super::transport_vehicle_movement::SkyStationCrossing,
            )>();
            close_owner(vehicle, &members, &mut circuits);
            if let Ok(riders) = riders.get(vehicle) {
                dismount(riders.iter(), &mut commands);
            }
        }
    }
}

fn close_disconnected_track(removed: On<Remove<(TrackStart, TrackEnd)>>, mut commands: Commands) {
    commands
        .entity(removed.entity)
        .try_remove::<(TrackSegment, TrackStart, TrackEnd, CircuitMember)>();
}

fn close_owner(
    entity: Entity,
    members: &Query<&CircuitMember>,
    circuits: &mut Query<&mut TransportCircuit>,
) {
    if let Ok(member) = members.get(entity) {
        if let Ok(mut circuit) = circuits.get_mut(member.0) {
            if !circuit.closed {
                circuit.closed = true;
            }
        }
    }
}

fn dismount(riders: impl Iterator<Item = Entity>, commands: &mut Commands) {
    for rider in riders {
        commands.entity(rider).try_remove::<TransportRider>();
    }
}

fn return_rider_to_ground(
    removed: On<Remove<TransportRider>>,
    mut modes: Query<&mut LocomotionMode>,
    mut commands: Commands,
) {
    if let Ok(mut mode) = modes.get_mut(removed.entity) {
        mode.set_if_neq(LocomotionMode::Ground);
    }
    commands.entity(removed.entity).try_remove::<(
        BoardedAtStation,
        TransportSeatRider,
        TourScore,
        super::tour_view_scoring::RideTourContributions,
        TourObservationMemory,
        super::tour_view_scoring::TourObservationDwell,
        PlayerRidingSeatIndex,
    )>();
}
