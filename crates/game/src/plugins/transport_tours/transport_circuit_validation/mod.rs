use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

use super::{
    topology::calculate_authored_transport_station_endpoint_world_position,
    transport_circuit_types::{CircuitMember, OpenCircuitRequest, TransportCircuit},
    transport_fact_validation::{
        authored_transport_track_piece_chain_is_valid, transport_station_capacity_is_valid,
        transport_track_segment_is_valid, transport_vehicle_capacity_and_speed_are_valid,
    },
    transport_topology_types::{TrackSegment, TransportStation, TransportTrackJunction},
    transport_vehicle_types::TransportVehicle,
};

fn transport_circuit_is_valid_for_opening(
    transport_circuit_entity: Entity,
    transport_circuit: &TransportCircuit,
    world_definitions: WorldDefinitionsView<'_>,
    endpoint_transforms: &Query<&GlobalTransform>,
    transport_circuit_members: &Query<(
        Entity,
        &CircuitMember,
        Option<&TrackSegment>,
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
    ) in transport_circuit_members.iter()
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
            for (_, track_circuit_member, track_segment, _, _, _) in
                transport_circuit_members.iter()
            {
                let Some(track_segment) = track_segment.filter(|track_segment| {
                    track_circuit_member.0 == transport_circuit_entity
                        && (track_segment.from == entity || track_segment.to == entity)
                }) else {
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
            if !transport_track_segment_is_valid(track_segment) {
                return false;
            }
            let Ok((_, start_circuit_member, _, start_station, start_junction, _)) =
                transport_circuit_members.get(track_segment.from)
            else {
                return false;
            };
            let Ok((_, end_circuit_member, _, end_station, end_junction, _)) =
                transport_circuit_members.get(track_segment.to)
            else {
                return false;
            };
            if start_circuit_member.0 != transport_circuit_entity
                || end_circuit_member.0 != transport_circuit_entity
            {
                return false;
            }
            let Ok(start_transform) = endpoint_transforms.get(track_segment.from) else {
                return false;
            };
            let Ok(end_transform) = endpoint_transforms.get(track_segment.to) else {
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

    // Every station or track junction in a valid directed circuit has exactly
    // one incoming and one outgoing track segment.
    for (endpoint_entity, circuit_member, _, transport_station, transport_junction, _) in
        transport_circuit_members.iter()
    {
        if circuit_member.0 != transport_circuit_entity
            || (transport_station.is_none() && transport_junction.is_none())
        {
            continue;
        }
        let mut incoming_track_count = 0usize;
        let mut outgoing_track_count = 0usize;
        for (_, track_circuit_member, track_segment, _, _, _) in transport_circuit_members.iter() {
            let Some(track_segment) =
                track_segment.filter(|_| track_circuit_member.0 == transport_circuit_entity)
            else {
                continue;
            };
            incoming_track_count += usize::from(track_segment.to == endpoint_entity);
            outgoing_track_count += usize::from(track_segment.from == endpoint_entity);
        }
        if incoming_track_count != 1 || outgoing_track_count != 1 {
            return false;
        }
    }

    // Degree validity permits disjoint loops, so walk exactly one full circuit.
    let mut current_endpoint_entity = first_station_entity;
    for traversed_track_count in 0..track_segment_count {
        let next_station_entity = transport_circuit_members.iter().find_map(
            |(_, circuit_member, track_segment, _, _, _)| {
                track_segment
                    .filter(|track_segment| {
                        circuit_member.0 == transport_circuit_entity
                            && track_segment.from == current_endpoint_entity
                    })
                    .map(|track_segment| track_segment.to)
            },
        );
        let Some(next_station_entity) = next_station_entity else {
            return false;
        };
        current_endpoint_entity = next_station_entity;
        if current_endpoint_entity == first_station_entity {
            return traversed_track_count + 1 == track_segment_count;
        }
    }
    false
}

pub(super) fn close_changed_transport_circuits_and_apply_open_requests(
    mut open_circuit_requests: MessageReader<OpenCircuitRequest>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut transport_circuits: Query<(Entity, &mut TransportCircuit)>,
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
        Option<&TrackSegment>,
        Option<&TransportStation>,
        Option<&TransportTrackJunction>,
        Option<&TransportVehicle>,
    )>,
    endpoint_transforms: Query<&GlobalTransform>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for circuit_member in &changed_circuit_members {
        if let Ok((_, mut transport_circuit)) = transport_circuits.get_mut(circuit_member.0) {
            transport_circuit.closed = true;
        }
    }
    for open_circuit_request in open_circuit_requests.read() {
        let Ok((transport_circuit_entity, mut transport_circuit)) =
            transport_circuits.get_mut(open_circuit_request.circuit)
        else {
            continue;
        };
        transport_circuit.closed = !open_circuit_request.open
            || !transport_circuit_is_valid_for_opening(
                transport_circuit_entity,
                &transport_circuit,
                world_definitions,
                &endpoint_transforms,
                &transport_circuit_members,
            );
    }
}
