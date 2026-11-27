use super::ground_track_policy::TrackTerrain;
use bevy::{
    ecs::entity::{EntityHashMap, EntityHashSet},
    platform::collections::HashSet,
    prelude::*,
};
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::transport_tours::transport_topology_types::{TrackEnd, TrackStart};
use openzt2_zoo_state::transport_tours::transport_topology_types::{TrackIncidence, TrackRoute};

use openzt2_game_assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

use crate::transport_tours::transport_fact_validation::authored_transport_track_piece_chain_is_valid;
use crate::transport_tours::transport_fact_validation::transport_station_capacity_is_valid;
use crate::transport_tours::transport_route_position_calculations::{
    construct_native_ground_transport_track_travel_path_points, TrackTravelDistances,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::transport_tours::transport_circuit_types::TransportCircuit;
use openzt2_zoo_state::transport_tours::transport_circuit_types::{CircuitMember, CircuitMembers};
use openzt2_zoo_state::transport_tours::transport_topology_types::AttachTransportStationRequest;
use openzt2_zoo_state::transport_tours::transport_topology_types::ConnectTransportTrackRequest;
use openzt2_zoo_state::transport_tours::transport_topology_types::DetachTransportStationRequest;
use openzt2_zoo_state::transport_tours::transport_topology_types::DisconnectTransportTrackRequest;
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackProfile;
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackSegment;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportConnectionFailure;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStationAssignmentRejected;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackConnectionApplied;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackConnectionRejected;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackDisconnectionApplied;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackJunction;

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
    segments: &Query<(Entity, &CircuitMember, TrackRoute)>,
    incidence: &TrackIncidence,
) -> bool {
    incidence
        .incident(endpoint)
        .filter_map(|entity| segments.get(entity).ok())
        .all(|(entity, _, segment)| {
            entity == ignored_track
                || if outgoing {
                    segment.from() != endpoint || segment.from_endpoint_index != endpoint_index
                } else {
                    segment.to() != endpoint || segment.to_endpoint_index != endpoint_index
                }
        })
}

#[derive(Message)]
pub(super) struct TransportEdgesDisconnected(pub(super) Entity);

pub(super) fn disconnect_tracks(
    mut requests: MessageReader<DisconnectTransportTrackRequest>,
    tracks: Query<(&CircuitMember, TrackRoute)>,
    mut circuits: Query<&mut TransportCircuit>,
    mut disconnected_tracks: MessageWriter<TransportTrackDisconnectionApplied>,
    mut split: MessageWriter<TransportEdgesDisconnected>,
    mut commands: Commands,
) {
    for request in requests.read() {
        if let Ok((member, _)) = tracks.get(request.track) {
            if let Ok(mut circuit) = circuits.get_mut(member.0) {
                circuit.closed = true;
                split.write(TransportEdgesDisconnected(member.0));
            }
            commands.entity(request.track).remove::<(
                TrackSegment,
                TrackTravelDistances,
                TrackStart,
                TrackEnd,
                CircuitMember,
            )>();
        }
        disconnected_tracks.write(TransportTrackDisconnectionApplied {
            transaction: request.transaction,
            application: request.application,
            track: request.track,
        });
    }
}

pub(super) fn attach_stations(
    mut requests: MessageReader<AttachTransportStationRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut circuits: Query<&mut TransportCircuit>,
    stations: Query<(&TransportStation, Option<&CircuitMember>)>,
    mut rejected_stations: MessageWriter<TransportStationAssignmentRejected>,
    mut commands: Commands,
) {
    let mut accepted = EntityHashSet::default();
    for request in requests.read() {
        let Some(definitions) = active_definitions.get(&definitions) else {
            rejected_stations.write(TransportStationAssignmentRejected {
                station: request.station,
                reason: TransportConnectionFailure::IncompatibleDefinition,
            });
            continue;
        };
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
        if member.is_some() || accepted.contains(&request.station) {
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
        accepted.insert(request.station);
        circuit.closed = true;
        commands
            .entity(request.station)
            .insert(CircuitMember(request.circuit));
    }
}

pub(super) fn detach_stations(
    mut requests: MessageReader<DetachTransportStationRequest>,
    mut circuits: Query<&mut TransportCircuit>,
    stations: Query<(&TransportStation, Option<&CircuitMember>)>,
    incidence: TrackIncidence,
    segments: Query<(), With<TrackSegment>>,
    mut rejected_stations: MessageWriter<TransportStationAssignmentRejected>,
    mut commands: Commands,
) {
    for request in requests.read() {
        let Ok((_, Some(member))) = stations.get(request.station) else {
            continue;
        };
        if incidence
            .incident(request.station)
            .any(|track| segments.contains(track))
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
}

pub(super) fn connect_tracks(
    mut requests: MessageReader<ConnectTransportTrackRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut circuits: Query<&mut TransportCircuit>,
    circuit_members: Query<&CircuitMembers>,
    (worlds, terrain): (Query<&WorldMember>, TrackTerrain),
    tracks: Query<(Option<&CircuitMember>, Has<TrackSegment>, &TrackProfile)>,
    stations: Query<(&TransportStation, Option<&CircuitMember>)>,
    junctions: Query<(Option<&CircuitMember>, &GlobalTransform), With<TransportTrackJunction>>,
    points: Query<&GlobalTransform>,
    incidence: TrackIncidence,
    segments: Query<(Entity, &CircuitMember, TrackRoute)>,
    mut rejected_tracks: MessageWriter<TransportTrackConnectionRejected>,
    mut applied_tracks: MessageWriter<TransportTrackConnectionApplied>,
    mut commands: Commands,
) {
    let mut tracks_reserved = EntityHashSet::default();
    let mut sockets_reserved = HashSet::new();
    let mut junction_members = EntityHashMap::default();
    let mut joined = EntityHashMap::default();
    let mut moves = EntityHashMap::default();
    for request in requests.read() {
        let Some(definitions) = active_definitions.get(&definitions) else {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::IncompatibleDefinition,
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
        if member.is_some() || segment || tracks_reserved.contains(&request.track) {
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
            .or_else(|| from_junction.and_then(|(member, _)| member.copied()))
            .or_else(|| junction_members.get(&request.from).copied());
        let to_member = to_station
            .and_then(|(_, member)| member.copied())
            .or_else(|| to_junction.and_then(|(member, _)| member.copied()))
            .or_else(|| junction_members.get(&request.to).copied());
        let circuit_entity = current_circuit(&joined, request.circuit);
        let other = to_member
            .map(|member| current_circuit(&joined, member.0))
            .filter(|&other| other != circuit_entity);
        let world = worlds.get(circuit_entity).ok().map(|member| member.root);
        let same_world = |entity| worlds.get(entity).ok().map(|member| member.root) == world;
        if world.is_none()
            || !same_world(request.track)
            || !same_world(request.from)
            || !same_world(request.to)
            || other.is_some_and(|other| !same_world(other))
        {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::ForeignEndpoint,
            });
            continue;
        }
        let Ok(mut circuit) = circuits.get_mut(circuit_entity) else {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::MissingCircuit,
            });
            continue;
        };
        if (from_station.is_none() && from_junction.is_none())
            || (to_station.is_none() && to_junction.is_none())
            || from_member.is_none_or(|member| current_circuit(&joined, member.0) != circuit_entity)
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
            // A road left after a gap starts at a junction nothing leads into
            // yet; any road that does lead into it must be of the same kind.
        }) || (from_junction.is_some()
            && incidence
                .incoming(request.from)
                .filter_map(|entity| segments.get(entity).ok())
                .any(|(track_entity, member, segment)| {
                    current_circuit(&joined, member.0) == circuit_entity
                        && segment.to() == request.from
                        && tracks
                            .get(track_entity)
                            .is_ok_and(|(_, _, adjacent_profile)| {
                                adjacent_profile.kind != profile.kind
                            })
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
        let Some((distances, length)) = TrackTravelDistances::calculate(&travel_path_points) else {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::InvalidGeometry,
            });
            continue;
        };
        if request.from == request.to
            || request.from_endpoint_index >= 2
            || request.to_endpoint_index >= 2
            || request.path_points.first() != Some(&from)
            || request.path_points.last() != Some(&to)
            || !authored_transport_track_piece_chain_is_valid(authored_track, &request.path_points)
            || (profile.kind == openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind::Ground
                && matches!(request.application, EditApplication::InitialCommit | EditApplication::Redo)
                && authored_track.ground_track_physics.as_ref().is_some_and(|policy| !world.is_some_and(|root| {
                    terrain.across_grade_allowed(
                        root, &request.path_points, [from_station.is_some(), to_station.is_some()], policy,
                    )
                })))
        {
            rejected_tracks.write(TransportTrackConnectionRejected {
                transaction: request.transaction,
                application: request.application,
                track: request.track,
                reason: TransportConnectionFailure::InvalidGeometry,
            });
            continue;
        }
        let outgoing = (request.from, request.from_endpoint_index, true);
        let incoming = (request.to, request.to_endpoint_index, false);
        if sockets_reserved.contains(&outgoing)
            || sockets_reserved.contains(&incoming)
            || !transport_circuit_endpoint_is_available_for_track_connection(
                request.from,
                request.from_endpoint_index,
                true,
                request.track,
                &segments,
                &incidence,
            )
            || !transport_circuit_endpoint_is_available_for_track_connection(
                request.to,
                request.to_endpoint_index,
                false,
                request.track,
                &segments,
                &incidence,
            )
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
        if let Some(other) = other {
            if let Ok(members) = circuit_members.get(other) {
                moves.extend(members.iter().map(|member| (member, other)));
            }
            joined.insert(other, circuit_entity);
        }
        moves.insert(request.track, circuit_entity);
        tracks_reserved.insert(request.track);
        sockets_reserved.extend([outgoing, incoming]);

        if profile.kind == openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind::Ground {
            commands.entity(request.track).insert(distances);
        }
        commands.entity(request.track).insert((
            CircuitMember(circuit_entity),
            TrackStart(request.from),
            TrackEnd(request.to),
            TrackSegment {
                definition: profile.definition,

                from_endpoint_index: request.from_endpoint_index,
                from_position: from,

                to_endpoint_index: request.to_endpoint_index,
                to_position: to,
                path_points: request.path_points.clone(),
                travel_path_points,
                length,
            },
        ));
        if to_junction.is_some() && to_member.is_none() {
            junction_members.insert(request.to, CircuitMember(circuit_entity));
            moves.insert(request.to, circuit_entity);
            commands
                .entity(request.to)
                .insert(CircuitMember(circuit_entity));
        }
        applied_tracks.write(TransportTrackConnectionApplied {
            transaction: request.transaction,
            application: request.application,
            track: request.track,
        });
    }
    for (member, original) in moves {
        let circuit = current_circuit(&joined, original);
        if circuit != original {
            commands.entity(member).insert(CircuitMember(circuit));
        }
    }
    for circuit in joined.into_keys() {
        commands.entity(circuit).despawn();
    }
}

fn current_circuit(joined: &EntityHashMap<Entity>, mut circuit: Entity) -> Entity {
    while let Some(&joined_to) = joined.get(&circuit) {
        circuit = joined_to;
    }
    circuit
}
