use bevy::prelude::*;

use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_interface_state::immersive_modes::immersive_mode_state_types::ControlledEntity;
use openzt2_zoo_state::{
    economy::facility_economy_types::{Price, Wallet},
    guests::guest_simulation_types::Guest,
    locomotion::locomotion_types::{
        Arrived, Destination, Docking, LocomotionMode, NavAgent, Route, Steering,
    },
    transport_tours::{
        tour_scoring_types::{TourObservationCadence, TourObservationMemory, TourScore},
        transport_circuit_types::{CircuitMember, TransportCircuit},
        transport_rider_types::*,
        transport_topology_types::TransportStation,
        transport_vehicle_types::TransportVehicle,
    },
};

pub(super) fn accept_affordable_transport_boarding_requests_at_reachable_stations(
    mut requests: MessageReader<BoardTransportRequest>,
    mut commands: Commands,
) {
    for request in requests.read().copied() {
        commands.run_system_cached_with(accept_boarding_request, request);
    }
}

fn accept_boarding_request(
    In(request): In<BoardTransportRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    guests: Query<
        (&Wallet, &GlobalTransform, &NavAgent),
        (
            With<Guest>,
            Without<WaitingForTransport>,
            Without<TransportRider>,
        ),
    >,
    stations: Query<(
        &CircuitMember,
        &TransportStation,
        &GlobalTransform,
        Option<&StationQueue>,
        Option<&StationBoardedRiders>,
    )>,
    circuits: Query<(&TransportCircuit, Option<&Price>)>,
    mut commands: Commands,
) {
    let (
        Some(definitions),
        Ok((wallet, guest_pose, agent)),
        Ok((member, station, pose, queue, boarded)),
    ) = (
        active_definitions.get(&definitions),
        guests.get(request.guest),
        stations.get(request.station),
    )
    else {
        return;
    };
    let (Ok((circuit, fare)), Some(authored_station)) = (
        circuits.get(member.0),
        definitions.find_station(station.definition),
    ) else {
        return;
    };
    let occupants =
        queue.map_or(0, |queue| queue.len()) + boarded.map_or(0, |boarded| boarded.len());
    if circuit.closed
        || occupants >= usize::from(station.capacity)
        || !fare.is_some_and(|fare| fare.0 .0 >= 0 && wallet.0 >= fare.0)
    {
        return;
    }
    let reaches_dock = authored_station
        .passenger_entry_offsets_cm
        .into_iter()
        .map(|[x, z]| pose.transform_point(Vec3::new(f32::from(x), 0.0, f32::from(z)) * 0.01))
        .any(|dock| {
            guest_pose.translation().distance_squared(dock) <= agent.radius_m * agent.radius_m
        });
    if reaches_dock {
        commands.entity(request.guest).insert(WaitingForTransport {
            station: request.station,
        });
    }
}

pub(super) fn board_waiting_guests_when_transport_vehicle_arrives(
    mut arrivals: MessageReader<Arrived>,
    mut commands: Commands,
) {
    for arrival in arrivals.read() {
        if let Some(station) = arrival.target {
            commands.run_system_cached_with(board_waiting_guests, (arrival.entity, station));
        }
    }
}

fn board_waiting_guests(
    In((vehicle_entity, station_entity)): In<(Entity, Entity)>,
    vehicles: Query<(&CircuitMember, &Children), With<TransportVehicle>>,
    stations: Query<(&CircuitMember, &StationQueue), With<TransportStation>>,
    mounts: Query<(Entity, &TransportSeat), Without<RidersAtSeat>>,
    mut guests: Query<
        (Has<ControlledEntity>, Option<&mut LocomotionMode>),
        (
            With<Guest>,
            With<WaitingForTransport>,
            Without<TransportRider>,
        ),
    >,
    mut commands: Commands,
) {
    let (Ok((vehicle_circuit, seats)), Ok((station_circuit, queue))) =
        (vehicles.get(vehicle_entity), stations.get(station_entity))
    else {
        return;
    };
    if station_circuit.0 != vehicle_circuit.0 {
        return;
    }
    let mut free_seats = mounts
        .iter_many(seats.iter())
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    free_seats.sort_unstable_by_key(|(_, seat)| seat.index);
    let mut waiting = queue
        .iter()
        .filter(|&guest| guests.contains(guest))
        .collect::<Vec<_>>();
    waiting.sort_unstable_by_key(|guest| guest.to_bits());
    for (guest, (mount, seat)) in waiting.into_iter().zip(free_seats) {
        let Ok((controlled, mode)) = guests.get_mut(guest) else {
            continue;
        };
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
                TourObservationMemory::default(),
                TourObservationCadence::default(),
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
