use super::topology::calculate_authored_transport_station_endpoint_world_position;
use super::transport_circuit_types::CircuitMember;
use super::transport_topology_types::TrackProfile;
use super::transport_topology_types::TrackSegment;
use super::transport_topology_types::TransportStation;
use super::transport_topology_types::TransportTrackJunction;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;

const TRANSPORT_STATION_ENDPOINT_POINTER_SNAP_DISTANCE_METRES: f32 = 8.0;

fn transport_station_endpoint_is_available(
    station: Entity,
    endpoint_index: u8,
    outgoing: bool,
    circuit: Entity,
    segments: &Query<(Entity, &CircuitMember, &TrackSegment, &TrackProfile)>,
) -> bool {
    segments.iter().all(|(_, member, segment, _)| {
        member.0 != circuit
            || if outgoing {
                segment.from != station || segment.from_endpoint_index != endpoint_index
            } else {
                segment.to != station || segment.to_endpoint_index != endpoint_index
            }
    })
}

#[allow(clippy::type_complexity)]
pub(super) fn find_nearest_compatible_transport_station_endpoint(
    pointer_position: Vec3,
    wanted_kind: TransportationTrackKind,
    wanted_world_root: Entity,
    wanted_circuit: Option<Entity>,
    outgoing: bool,
    excluded_station: Option<Entity>,
    definitions: WorldDefinitionsView<'_>,
    stations: &Query<(
        Entity,
        &TransportStation,
        &CircuitMember,
        &GlobalTransform,
        &WorldMember,
    )>,
    segments: &Query<(Entity, &CircuitMember, &TrackSegment, &TrackProfile)>,
) -> Option<(Entity, Entity, u8, Vec3)> {
    stations
        .iter()
        .filter(|(entity, station, member, _, world_member)| {
            Some(*entity) != excluded_station
                && world_member.root == wanted_world_root
                && wanted_circuit.is_none_or(|circuit| member.0 == circuit)
                && definitions
                    .find_station(station.definition)
                    .is_some_and(|definition| definition.kind == wanted_kind)
        })
        .flat_map(|(entity, station, member, transform, _)| {
            (0_u8..2).filter_map(move |endpoint_index| {
                transport_station_endpoint_is_available(
                    entity,
                    endpoint_index,
                    outgoing,
                    member.0,
                    segments,
                )
                .then(|| {
                    calculate_authored_transport_station_endpoint_world_position(
                        definitions,
                        station,
                        transform,
                        endpoint_index,
                    )
                    .map(|position| (entity, member.0, endpoint_index, position))
                })
                .flatten()
            })
        })
        .filter_map(|endpoint| {
            let distance_squared = endpoint.3.distance_squared(pointer_position);
            (distance_squared <= TRANSPORT_STATION_ENDPOINT_POINTER_SNAP_DISTANCE_METRES.powi(2))
                .then_some((distance_squared, endpoint))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, endpoint)| endpoint)
}

#[allow(clippy::type_complexity)]
pub(super) fn find_nearest_compatible_transport_track_junction(
    pointer_position: Vec3,
    wanted_kind: TransportationTrackKind,
    wanted_world_root: Entity,
    wanted_circuit: Option<Entity>,
    junctions: &Query<(
        Entity,
        &TransportTrackJunction,
        &CircuitMember,
        &GlobalTransform,
        &WorldMember,
    )>,
    segments: &Query<(Entity, &CircuitMember, &TrackSegment, &TrackProfile)>,
) -> Option<(Entity, Entity, u8, Vec3)> {
    junctions
        .iter()
        .filter(|(entity, _, member, _, world_member)| {
            world_member.root == wanted_world_root
                && wanted_circuit.is_none_or(|circuit| member.0 == circuit)
                && segments.iter().any(|(_, track_member, segment, profile)| {
                    track_member.0 == member.0
                        && profile.kind == wanted_kind
                        && segment.to == *entity
                })
                && segments.iter().all(|(_, track_member, segment, _)| {
                    track_member.0 != member.0 || segment.from != *entity
                })
        })
        .filter_map(|(entity, _, member, transform, _)| {
            let position = transform.translation();
            let distance_squared = position.distance_squared(pointer_position);
            (distance_squared <= TRANSPORT_STATION_ENDPOINT_POINTER_SNAP_DISTANCE_METRES.powi(2))
                .then_some((distance_squared, (entity, member.0, 0, position)))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, endpoint)| endpoint)
}

pub(super) fn nearer_transport_track_construction_endpoint(
    pointer_position: Vec3,
    left: Option<(Entity, Entity, u8, Vec3)>,
    right: Option<(Entity, Entity, u8, Vec3)>,
) -> Option<(Entity, Entity, u8, Vec3)> {
    match (left, right) {
        (Some(left), Some(right)) => {
            if left.3.distance_squared(pointer_position)
                <= right.3.distance_squared(pointer_position)
            {
                Some(left)
            } else {
                Some(right)
            }
        }
        (left, right) => left.or(right),
    }
}
