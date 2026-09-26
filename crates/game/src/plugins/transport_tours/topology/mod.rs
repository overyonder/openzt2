use bevy::{ecs::system::SystemParam, prelude::*};

use crate::assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

use super::{
    transport_circuit_types::{CircuitMember, TransportCircuit},
    transport_fact_validation::{
        authored_transport_track_piece_chain_is_valid, transport_station_capacity_is_valid,
        transport_track_path_length,
    },
    transport_route_position_calculations::construct_native_ground_transport_track_travel_path_points,
    transport_topology_types::{
        AttachTransportStationRequest, ConnectTransportTrackRequest, DetachTransportStationRequest,
        DisconnectTransportTrackRequest, TrackProfile, TrackSegment, TransportConnectionFailure,
        TransportStation, TransportStationAssignmentRejected, TransportTrackConnectionApplied,
        TransportTrackConnectionRejected, TransportTrackDisconnectionApplied,
        TransportTrackJunction,
    },
};

#[derive(SystemParam)]
pub(super) struct TransportTopologyMutationMessageWriters<'w> {
    rejected_tracks: MessageWriter<'w, TransportTrackConnectionRejected>,
    applied_tracks: MessageWriter<'w, TransportTrackConnectionApplied>,
    disconnected_tracks: MessageWriter<'w, TransportTrackDisconnectionApplied>,
    rejected_stations: MessageWriter<'w, TransportStationAssignmentRejected>,
}

fn find_authored_transportation_kind_for_transport_station(
    definitions: WorldDefinitionsView<'_>,
    station: &TransportStation,
) -> Option<openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind>
{
    definitions
        .find_station(station.definition)
        .map(|definition| definition.kind)
}

pub(super) fn calculate_authored_transport_station_endpoint_world_position(
    definitions: WorldDefinitionsView<'_>,
    station: &TransportStation,
    station_transform: &GlobalTransform,
    endpoint_index: u8,
) -> Option<Vec3> {
    const STATION_ORIENTATION_ALIGNMENT_EPSILON: f32 = 0.001;

    let station_definition = definitions.find_station(station.definition)?;
    let station_forward = station_transform.affine().transform_vector3(Vec3::Z).xz();
    let station_is_diagonally_aligned = (station_forward.x.abs() - station_forward.y.abs()).abs()
        < STATION_ORIENTATION_ALIGNMENT_EPSILON;
    let endpoint_offsets = if station_is_diagonally_aligned {
        station_definition.diagonal_endpoint_offsets_metres?
    } else {
        station_definition.cardinal_endpoint_offsets_metres?
    };
    let local_position = Vec3::from_array(convert_source_z_up_vector_to_bevy_y_up_coordinates(
        endpoint_offsets.get(usize::from(endpoint_index)).copied()?,
    ));
    Some(station_transform.transform_point(local_position))
}

fn transport_circuit_endpoint_is_available_for_track_connection(
    endpoint: Entity,
    endpoint_index: u8,
    outgoing: bool,
    ignored_track: Entity,
    circuit: Entity,
    segments: &Query<(Entity, &CircuitMember, &TrackSegment)>,
) -> bool {
    segments.iter().all(|(entity, member, segment)| {
        entity == ignored_track
            || member.0 != circuit
            || if outgoing {
                segment.from != endpoint || segment.from_endpoint_index != endpoint_index
            } else {
                segment.to != endpoint || segment.to_endpoint_index != endpoint_index
            }
    })
}

/// Applies construction topology intents directly to the canonical edge
/// component. A successful edit closes the circuit until normal circuit
/// validation explicitly reopens it.
pub(super) fn apply_transport_station_and_track_topology_mutation_requests(
    mut station_attachments: MessageReader<AttachTransportStationRequest>,
    mut station_detachments: MessageReader<DetachTransportStationRequest>,
    mut disconnects: MessageReader<DisconnectTransportTrackRequest>,
    mut connections: MessageReader<ConnectTransportTrackRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut circuits: Query<&mut TransportCircuit>,
    tracks: Query<(Option<&CircuitMember>, Option<&TrackSegment>, &TrackProfile)>,
    stations: Query<(&TransportStation, Option<&CircuitMember>)>,
    junctions: Query<(Option<&CircuitMember>, &GlobalTransform), With<TransportTrackJunction>>,
    points: Query<&GlobalTransform>,
    segments: Query<(Entity, &CircuitMember, &TrackSegment)>,
    mutation_messages: TransportTopologyMutationMessageWriters,
    mut commands: Commands,
    mut claimed_endpoints: Local<Vec<(Entity, u8, bool)>>,
    mut processed_tracks: Local<Vec<Entity>>,
) {
    let TransportTopologyMutationMessageWriters {
        mut rejected_tracks,
        mut applied_tracks,
        mut disconnected_tracks,
        mut rejected_stations,
    } = mutation_messages;
    claimed_endpoints.clear();
    processed_tracks.clear();

    for request in disconnects.read() {
        if let Ok((member, segment, _)) = tracks.get(request.track) {
            if let Some(member) = member.filter(|_| segment.is_some()) {
                if let Ok(mut circuit) = circuits.get_mut(member.0) {
                    circuit.closed = true;
                }
                commands
                    .entity(request.track)
                    .remove::<(TrackSegment, CircuitMember)>();
            }
        }
        disconnected_tracks.write(TransportTrackDisconnectionApplied {
            transaction: request.transaction,
            application: request.application,
            track: request.track,
        });
    }

    let Some(definitions) = active_definitions.get(&definitions) else {
        for request in station_attachments.read() {
            rejected_stations.write(TransportStationAssignmentRejected {
                station: request.station,
                reason: TransportConnectionFailure::IncompatibleDefinition,
            });
        }
        for request in connections.read() {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::IncompatibleDefinition,
            });
        }
        return;
    };

    for request in station_attachments.read() {
        let Ok(mut circuit) = circuits.get_mut(request.circuit) else {
            rejected_stations.write(TransportStationAssignmentRejected {
                station: request.station,
                reason: TransportConnectionFailure::MissingCircuit,
            });
            continue;
        };
        let Ok((station, member)) = stations.get(request.station) else {
            rejected_stations.write(TransportStationAssignmentRejected {
                station: request.station,
                reason: TransportConnectionFailure::MissingStation,
            });
            continue;
        };
        if member.is_some() {
            rejected_stations.write(TransportStationAssignmentRejected {
                station: request.station,
                reason: TransportConnectionFailure::AlreadyMember,
            });
            continue;
        }
        if !transport_station_capacity_is_valid(station)
            || find_authored_transportation_kind_for_transport_station(definitions, station)
                .is_none()
        {
            rejected_stations.write(TransportStationAssignmentRejected {
                station: request.station,
                reason: TransportConnectionFailure::IncompatibleDefinition,
            });
            continue;
        }
        circuit.closed = true;
        commands
            .entity(request.station)
            .insert(CircuitMember(request.circuit));
    }

    for request in station_detachments.read() {
        let Ok((_, Some(member))) = stations.get(request.station) else {
            continue;
        };
        if segments
            .iter()
            .any(|(_, _, edge)| edge.from == request.station || edge.to == request.station)
        {
            rejected_stations.write(TransportStationAssignmentRejected {
                station: request.station,
                reason: TransportConnectionFailure::OccupiedEndpoint,
            });
            continue;
        }
        if let Ok(mut circuit) = circuits.get_mut(member.0) {
            circuit.closed = true;
        }
        commands.entity(request.station).remove::<CircuitMember>();
    }

    for request in connections.read() {
        if processed_tracks.contains(&request.track) {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::AlreadyConnected,
            });
            continue;
        }
        processed_tracks.push(request.track);
        let Ok(mut circuit) = circuits.get_mut(request.circuit) else {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::MissingCircuit,
            });
            continue;
        };
        let Ok((member, segment, profile)) = tracks.get(request.track) else {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::MissingTrack,
            });
            continue;
        };
        if member.is_some() || segment.is_some() {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::AlreadyConnected,
            });
            continue;
        }
        let from_station = stations.get(request.from).ok();
        let to_station = stations.get(request.to).ok();
        let from_junction = junctions.get(request.from).ok();
        let to_junction = junctions.get(request.to).ok();
        let from_member = from_station
            .and_then(|(_, member)| member.copied())
            .or_else(|| from_junction.and_then(|(member, _)| member.copied()));
        let to_member = to_station
            .and_then(|(_, member)| member.copied())
            .or_else(|| to_junction.and_then(|(member, _)| member.copied()));
        if (from_station.is_none() && from_junction.is_none())
            || (to_station.is_none() && to_junction.is_none())
            || from_member.is_none_or(|member| member.0 != request.circuit)
            || to_member.is_some_and(|member| member.0 != request.circuit)
        {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::ForeignEndpoint,
            });
            continue;
        }
        if from_station.is_some_and(|(station, _)| {
            !transport_station_capacity_is_valid(station)
                || find_authored_transportation_kind_for_transport_station(definitions, station)
                    != Some(profile.kind)
        }) || to_station.is_some_and(|(station, _)| {
            !transport_station_capacity_is_valid(station)
                || find_authored_transportation_kind_for_transport_station(definitions, station)
                    != Some(profile.kind)
        }) || (from_junction.is_some()
            && !segments.iter().any(|(track_entity, member, segment)| {
                member.0 == request.circuit
                    && segment.to == request.from
                    && tracks
                        .get(track_entity)
                        .is_ok_and(|(_, _, adjacent_profile)| adjacent_profile.kind == profile.kind)
            }))
        {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::IncompatibleDefinition,
            });
            continue;
        }
        let from = from_station
            .and_then(|(station, _)| {
                points.get(request.from).ok().and_then(|transform| {
                    calculate_authored_transport_station_endpoint_world_position(
                        definitions,
                        station,
                        transform,
                        request.from_endpoint_index,
                    )
                })
            })
            .or_else(|| from_junction.map(|(_, transform)| transform.translation()));
        let to = to_station
            .and_then(|(station, _)| {
                points.get(request.to).ok().and_then(|transform| {
                    calculate_authored_transport_station_endpoint_world_position(
                        definitions,
                        station,
                        transform,
                        request.to_endpoint_index,
                    )
                })
            })
            .or_else(|| to_junction.map(|(_, transform)| transform.translation()));
        let (Some(from), Some(to)) = (from, to) else {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::MissingEndpoint,
            });
            continue;
        };
        let Some(authored_track) = definitions.find_track(profile.definition) else {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::IncompatibleDefinition,
            });
            continue;
        };
        #[allow(
            clippy::suspicious_operation_groupings,
            reason = "the authored and runtime grade fields deliberately have different names"
        )]
        if authored_track.kind != profile.kind
            || authored_track.max_grade_permille != profile.maximum_grade_permille
        {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::IncompatibleDefinition,
            });
            continue;
        }
        let travel_path_points = if profile.kind
            == openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind::Ground
        {
            construct_native_ground_transport_track_travel_path_points(&request.path_points)
        } else {
            request.path_points.clone()
        };
        let Some(length) = transport_track_path_length(&travel_path_points) else {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::InvalidGeometry,
            });
            continue;
        };
        if request.from == request.to
            || request.path_points.first() != Some(&from)
            || request.path_points.last() != Some(&to)
            || !authored_transport_track_piece_chain_is_valid(authored_track, &request.path_points)
        {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::InvalidGeometry,
            });
            continue;
        }
        if !transport_circuit_endpoint_is_available_for_track_connection(
            request.from,
            request.from_endpoint_index,
            true,
            request.track,
            request.circuit,
            &segments,
        ) || !transport_circuit_endpoint_is_available_for_track_connection(
            request.to,
            request.to_endpoint_index,
            false,
            request.track,
            request.circuit,
            &segments,
        ) || claimed_endpoints.contains(&(request.from, request.from_endpoint_index, true))
            || claimed_endpoints.contains(&(request.to, request.to_endpoint_index, false))
        {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::OccupiedEndpoint,
            });
            continue;
        }

        circuit.closed = true;
        claimed_endpoints.push((request.from, request.from_endpoint_index, true));
        claimed_endpoints.push((request.to, request.to_endpoint_index, false));
        commands.entity(request.track).insert((
            CircuitMember(request.circuit),
            TrackSegment {
                definition: profile.definition,
                from: request.from,
                from_endpoint_index: request.from_endpoint_index,
                from_position: from,
                to: request.to,
                to_endpoint_index: request.to_endpoint_index,
                to_position: to,
                path_points: request.path_points.clone(),
                travel_path_points,
                length,
            },
        ));
        if to_junction.is_some() && to_member.is_none() {
            commands
                .entity(request.to)
                .insert(CircuitMember(request.circuit));
        }
        applied_tracks.write(TransportTrackConnectionApplied {
            transaction: request.transaction,
            application: request.application,
            track: request.track,
        });
    }
}
