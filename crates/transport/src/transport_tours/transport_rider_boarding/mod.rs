use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::prelude::*;

use openzt2_interface_state::immersive_modes::immersive_mode_state_types::ControlledEntity;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    economy::facility_economy_types::{Price, Wallet},
    guests::guest_simulation_types::Guest,
    locomotion::locomotion_types::{
        Arrived, Destination, Docking, LocomotionMode, Route, Steering,
    },
    transport_tours::{
        tour_scoring_types::{TourObservationMemory, TourScore},
        transport_circuit_types::{CircuitMember, TransportCircuit},
        transport_rider_types::*,
        transport_topology_types::TransportStation,
        transport_vehicle_types::TransportVehicle,
    },
};

pub(super) fn accept_transport_queue_reservations(
    mut requests: MessageReader<BoardTransportRequest>,
    guests: Query<
        (&Wallet, &WorldMember),
        (
            With<Guest>,
            Without<WaitingForTransport>,
            Without<TransportRider>,
        ),
    >,
    stations: Query<(
        &CircuitMember,
        &TransportStation,
        &WorldMember,
        Option<&StationQueue>,
        Option<&StationBoardedRiders>,
    )>,
    circuits: Query<(&TransportCircuit, Option<&Price>)>,
    mut commands: Commands,
) {
    let mut accepted_guests = EntityHashSet::default();
    let mut added_to_queue = EntityHashMap::<usize>::default();
    for request in requests.read() {
        if accepted_guests.contains(&request.guest) {
            continue;
        }
        let (Ok((wallet, guest_world)), Ok((member, station, station_world, queue, boarded))) =
            (guests.get(request.guest), stations.get(request.station))
        else {
            continue;
        };
        let Ok((circuit, fare)) = circuits.get(member.0) else {
            continue;
        };
        let occupants = queue.map_or(0, |queue| queue.len())
            + boarded.map_or(0, |boarded| boarded.len())
            + added_to_queue
                .get(&request.station)
                .copied()
                .unwrap_or_default();
        if guest_world.root != station_world.root
            || circuit.closed
            || occupants >= usize::from(station.capacity)
            || !fare.is_some_and(|fare| fare.0 .0 >= 0 && wallet.0 >= fare.0)
        {
            continue;
        }
        accepted_guests.insert(request.guest);
        *added_to_queue.entry(request.station).or_default() += 1;
        commands.entity(request.guest).insert(WaitingForTransport {
            station: request.station,
        });
    }
}

pub(super) fn board_waiting_guests_when_transport_vehicle_arrives(
    mut arrivals: MessageReader<Arrived>,
    vehicles: Query<(&CircuitMember, &Children), With<TransportVehicle>>,
    stations: Query<(&CircuitMember, &StationQueue), With<TransportStation>>,
    mounts: Query<(Entity, &TransportSeat), Without<RidersAtSeat>>,
    circuits: Query<(&TransportCircuit, &Price)>,
    mut guests: Query<
        (Has<ControlledEntity>, Option<&mut LocomotionMode>, &Wallet),
        (
            With<Guest>,
            With<WaitingForTransport>,
            Without<TransportRider>,
        ),
    >,
    mut commands: Commands,
) {
    let mut occupied_seats = EntityHashSet::default();
    let mut boarded_guests = EntityHashSet::default();
    for arrival in arrivals.read() {
        let vehicle_entity = arrival.entity;
        let Some(station_entity) = arrival.target else {
            continue;
        };
        let (Ok((vehicle_circuit, seats)), Ok((station_circuit, queue))) =
            (vehicles.get(vehicle_entity), stations.get(station_entity))
        else {
            continue;
        };
        let Ok((circuit, fare)) = circuits.get(station_circuit.0) else {
            continue;
        };
        if circuit.closed || station_circuit.0 != vehicle_circuit.0 {
            continue;
        }
        let mut free_seats = mounts
            .iter_many(seats.iter())
            .filter_map(Result::ok)
            .filter(|(mount, _)| !occupied_seats.contains(mount))
            .collect::<Vec<_>>();
        free_seats.sort_unstable_by_key(|(_, seat)| seat.index);
        let waiting = queue
            .iter()
            .filter(|&guest| !boarded_guests.contains(&guest))
            .filter(|&guest| {
                guests
                    .get(guest)
                    .is_ok_and(|(_, _, wallet)| fare.0 .0 >= 0 && wallet.0 >= fare.0)
            })
            .collect::<Vec<_>>();
        for (guest, (mount, seat)) in waiting.into_iter().zip(free_seats) {
            let Ok((controlled, mode, _)) = guests.get_mut(guest) else {
                continue;
            };
            boarded_guests.insert(guest);
            occupied_seats.insert(mount);
            let mut entity = commands.entity(guest);
            entity
                .remove::<(WaitingForTransport, Destination, Route, Docking, Steering)>()
                .insert((
                    TransportRider {
                        vehicle: vehicle_entity,
                    },
                    TransportSeatRider(mount),
                    BoardedAtStation(station_entity),
                    TourScore::default(),
                    super::tour_view_scoring::RideTourContributions::default(),
                    TourObservationMemory::default(),
                    super::tour_view_scoring::TourObservationDwell::default(),
                ));
            if let Some(mut mode) = mode {
                mode.set_if_neq(LocomotionMode::Vehicle);
            } else {
                entity.insert(LocomotionMode::Vehicle);
            }
            if controlled {
                entity.insert(PlayerRidingSeatIndex(seat.index));
            }
        }
    }
}
