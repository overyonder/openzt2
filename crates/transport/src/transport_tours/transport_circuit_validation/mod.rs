use bevy::prelude::*;
use openzt2_zoo_state::transport_tours::transport_topology_types::{TrackIncidence, TrackRoute};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

use crate::transport_tours::topology::calculate_authored_transport_station_endpoint_world_position;
use crate::transport_tours::transport_fact_validation::authored_transport_track_piece_chain_is_valid;
use crate::transport_tours::transport_fact_validation::transport_station_capacity_is_valid;
use crate::transport_tours::transport_fact_validation::transport_vehicle_capacity_and_speed_are_valid;
use crate::transport_tours::transport_route_position_calculations::TrackTravelDistances;
use openzt2_zoo_state::transport_tours::transport_circuit_types::OpenCircuitRequest;
use openzt2_zoo_state::transport_tours::transport_circuit_types::TransportCircuit;
use openzt2_zoo_state::transport_tours::transport_circuit_types::{CircuitMember, CircuitMembers};
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackSegment;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackJunction;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::TransportVehicle;

fn transport_circuit_is_valid_for_opening(
    transport_circuit_entity: Entity,
    transport_circuit: &TransportCircuit,
    world_definitions: WorldDefinitionsView<'_>,
    endpoint_transforms: &Query<&GlobalTransform>,
    members: &CircuitMembers,
    incidence: &TrackIncidence,
    valid_routes: &Query<(), With<TrackTravelDistances>>,
    transport_circuit_members: &Query<(
        Entity,
        &CircuitMember,
        Option<TrackRoute>,
        Option<&TransportStation>,
        Option<&TransportTrackJunction>,
        Option<&TransportVehicle>,
    )>,
) -> bool {
    let Some(transport_kind) = world_definitions
        .find_station(transport_circuit.definition)
        .map(|station| station.kind)
        .or_else(|| {
            world_definitions
                .find_track(transport_circuit.definition)
                .map(|track| track.kind)
        })
    else {
        return false;
    };
    let mut station_count = 0usize;
    let mut track_segment_count = 0usize;
    let mut junction_count = 0usize;
    let mut vehicle_count = 0usize;
    let mut first_station_entity = None;

    for (
        entity,
        circuit_member,
        track_segment,
        transport_station,
        transport_junction,
        transport_vehicle,
    ) in transport_circuit_members
        .iter_many(members.iter())
        .filter_map(Result::ok)
    {
        if circuit_member.0 != transport_circuit_entity {
            continue;
        }
        if let Some(transport_station) = transport_station {
            let Some(authored_station) =
                world_definitions.find_station(transport_station.definition)
            else {
                return false;
            };
            if !transport_station_capacity_is_valid(transport_station)
                || transport_station.capacity != authored_station.capacity
            {
                return false;
            }
            station_count += 1;
            first_station_entity.get_or_insert(entity);
            for (_, track_circuit_member, track_segment, _, _, _) in transport_circuit_members
                .iter_many(incidence.incident(entity))
                .filter_map(Result::ok)
            {
                let Some(track_segment) =
                    track_segment.filter(|_| track_circuit_member.0 == transport_circuit_entity)
                else {
                    continue;
                };
                let Some(authored_track) = world_definitions.find_track(track_segment.definition)
                else {
                    return false;
                };
                if authored_station.kind != authored_track.kind {
                    return false;
                }
            }
        }
        junction_count += usize::from(transport_junction.is_some());
        if let Some(track_segment) = track_segment {
            let Some(authored_track) = world_definitions.find_track(track_segment.definition)
            else {
                return false;
            };
            if track_segment.from() == track_segment.to() || !valid_routes.contains(entity) {
                return false;
            }
            let Ok((_, start_circuit_member, _, start_station, start_junction, _)) =
                transport_circuit_members.get(track_segment.from())
            else {
                return false;
            };
            let Ok((_, end_circuit_member, _, end_station, end_junction, _)) =
                transport_circuit_members.get(track_segment.to())
            else {
                return false;
            };
            if start_circuit_member.0 != transport_circuit_entity
                || end_circuit_member.0 != transport_circuit_entity
            {
                return false;
            }
            let Ok(start_transform) = endpoint_transforms.get(track_segment.from()) else {
                return false;
            };
            let Ok(end_transform) = endpoint_transforms.get(track_segment.to()) else {
                return false;
            };
            let start_position = start_station
                .and_then(|station| {
                    calculate_authored_transport_station_endpoint_world_position(
                        world_definitions,
                        station,
                        start_transform,
                        track_segment.from_endpoint_index,
                    )
                })
                .or_else(|| start_junction.map(|_| start_transform.translation()));
            let end_position = end_station
                .and_then(|station| {
                    calculate_authored_transport_station_endpoint_world_position(
                        world_definitions,
                        station,
                        end_transform,
                        track_segment.to_endpoint_index,
                    )
                })
                .or_else(|| end_junction.map(|_| end_transform.translation()));
            let (Some(start_position), Some(end_position)) = (start_position, end_position) else {
                return false;
            };
            if start_position != track_segment.from_position
                || end_position != track_segment.to_position
                || !authored_transport_track_piece_chain_is_valid(
                    authored_track,
                    &track_segment.path_points,
                )
            {
                return false;
            }
            track_segment_count += 1;
        }
        if let Some(transport_vehicle) = transport_vehicle {
            let Some(authored_vehicle) =
                world_definitions.find_vehicle(transport_vehicle.definition)
            else {
                return false;
            };
            let authored_speed = authored_vehicle.maximum_speed_metres_per_second;
            if !transport_vehicle_capacity_and_speed_are_valid(transport_vehicle)
                || authored_vehicle.kind != transport_kind
                || transport_vehicle.seats != authored_vehicle.seats
                || transport_vehicle.maximum_speed_metres_per_second.to_bits()
                    != authored_speed.to_bits()
            {
                return false;
            }
            vehicle_count += 1;
        }
    }

    let Some(first_station_entity) = first_station_entity else {
        return false;
    };
    if station_count < 2
        || station_count + junction_count != track_segment_count
        || vehicle_count == 0
    {
        return false;
    }

    let mut successors = bevy::ecs::entity::EntityHashMap::default();
    let in_circuit = |segment: Entity| {
        transport_circuit_members
            .get(segment)
            .ok()
            .and_then(|(_, member, route, _, _, _)| {
                (member.0 == transport_circuit_entity)
                    .then_some(route)
                    .flatten()
            })
    };
    for (endpoint, member, _, station, junction, _) in transport_circuit_members
        .iter_many(members.iter())
        .filter_map(Result::ok)
    {
        if member.0 != transport_circuit_entity || (station.is_none() && junction.is_none()) {
            continue;
        }
        let mut incoming = incidence.incoming(endpoint).filter_map(in_circuit);
        let mut outgoing = incidence.outgoing(endpoint).filter_map(in_circuit);
        let (Some(_), Some(next)) = (incoming.next(), outgoing.next()) else {
            return false;
        };
        if incoming.next().is_some() || outgoing.next().is_some() {
            return false;
        }
        successors.insert(endpoint, next.to());
    }

    // Degree validity permits disjoint loops, so walk exactly one full circuit.
    let mut endpoint = first_station_entity;
    for traversed in 0..track_segment_count {
        let Some(&next) = successors.get(&endpoint) else {
            return false;
        };
        endpoint = next;
        if endpoint == first_station_entity {
            return traversed + 1 == track_segment_count;
        }
    }

    false
}

pub(super) fn close_changed_transport_circuits_and_apply_open_requests(
    mut open_circuit_requests: MessageReader<OpenCircuitRequest>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut transport_circuits: Query<(Entity, &mut TransportCircuit, Option<&CircuitMembers>)>,
    changed_circuit_members: Query<
        &CircuitMember,
        Or<(
            Changed<CircuitMember>,
            Changed<TrackSegment>,
            Changed<TransportStation>,
            Changed<TransportTrackJunction>,
            Changed<TransportVehicle>,
        )>,
    >,
    transport_circuit_members: Query<(
        Entity,
        &CircuitMember,
        Option<TrackRoute>,
        Option<&TransportStation>,
        Option<&TransportTrackJunction>,
        Option<&TransportVehicle>,
    )>,
    endpoint_transforms: Query<&GlobalTransform>,
    incidence: TrackIncidence,
    valid_routes: Query<(), With<TrackTravelDistances>>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for circuit_member in &changed_circuit_members {
        if let Ok((_, mut transport_circuit, _)) = transport_circuits.get_mut(circuit_member.0) {
            transport_circuit.closed = true;
        }
    }
    for open_circuit_request in open_circuit_requests.read() {
        let Ok((transport_circuit_entity, mut transport_circuit, members)) =
            transport_circuits.get_mut(open_circuit_request.circuit)
        else {
            continue;
        };
        transport_circuit.closed = !open_circuit_request.open
            || !members.is_some_and(|members| {
                transport_circuit_is_valid_for_opening(
                    transport_circuit_entity,
                    &transport_circuit,
                    world_definitions,
                    &endpoint_transforms,
                    members,
                    &incidence,
                    &valid_routes,
                    &transport_circuit_members,
                )
            });
    }
}
