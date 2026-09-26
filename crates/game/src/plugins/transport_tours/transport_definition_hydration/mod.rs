use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::locomotion::locomotion_types::LocomotionMode;
use crate::plugins::locomotion::locomotion_types::Velocity;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

use super::{
    tour_scoring_types::{StationRatingPermille, TourViewable},
    transport_circuit_types::{CircuitDirection, CircuitMember, TransportCircuit},
    transport_fact_validation::transport_track_segment_is_valid,
    transport_topology_types::{TrackProfile, TrackSegment, TransportStation},
    transport_vehicle_types::{RoutePosition, StationDestination, TransportVehicle},
};

#[derive(Component)]
pub(super) struct TransportDefinitionResolved;

/// Projects resolved catalogue facts onto the ordinary entities that own them.
/// Topology is deliberately not copied: construction supplies `CircuitMember`
/// and `TrackSegment` entity relations when a route is joined.
pub(super) fn hydrate_transport_entities_from_authored_world_definitions(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    unresolved_transport_entities: Query<
        (Entity, &DefinitionId),
        Without<TransportDefinitionResolved>,
    >,
    mut commands: Commands,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (entity, definition_identifier) in &unresolved_transport_entities {
        let station_definition = world_definitions.stations().find(|definition| {
            AssetId(definition.object.0) == definition_identifier.0
                || AssetId(definition.id.0) == definition_identifier.0
        });
        if let Some(station_definition) = station_definition {
            commands.entity(entity).insert((
                TransportStation {
                    definition: AssetId(station_definition.id.0),
                    capacity: station_definition.capacity,
                    queued: 0,
                    occupied: 0,
                },
                StationRatingPermille::default(),
            ));
        }

        let track_definition = world_definitions.tracks().find(|definition| {
            AssetId(definition.object.0) == definition_identifier.0
                || AssetId(definition.id.0) == definition_identifier.0
        });
        if let Some(track_definition) = track_definition {
            commands.entity(entity).insert(TrackProfile {
                definition: AssetId(track_definition.id.0),
                kind: track_definition.kind,
                maximum_grade_permille: track_definition.max_grade_permille,
            });
        }

        let vehicle_definition = world_definitions.vehicles().find(|definition| {
            AssetId(definition.object.0) == definition_identifier.0
                || AssetId(definition.id.0) == definition_identifier.0
        });
        if let Some(vehicle_definition) = vehicle_definition {
            commands.entity(entity).insert((
                TransportVehicle {
                    definition: AssetId(vehicle_definition.id.0),
                    seats: vehicle_definition.seats,
                    occupied: 0,
                    maximum_speed_metres_per_second: vehicle_definition
                        .maximum_speed_metres_per_second,
                },
                LocomotionMode::Vehicle,
                Velocity::default(),
            ));
        }

        let tour_view_definition = world_definitions.tour_views().find(|definition| {
            AssetId(definition.subject.0) == definition_identifier.0
                || AssetId(definition.id.0) == definition_identifier.0
        });
        if let Some(tour_view_definition) = tour_view_definition {
            commands.entity(entity).insert(TourViewable {
                definition: AssetId(tour_view_definition.id.0),
            });
        }
        commands.entity(entity).insert(TransportDefinitionResolved);
    }
}

/// Places newly hydrated vehicles on the first stable edge of their circuit.
///
/// Route ownership remains on `TrackSegment` entities; this stores only the
/// vehicle's current edge and scalar position. A vehicle whose circuit has no
/// complete edge remains inert instead of receiving a synthetic route.
pub(super) fn initialize_transport_vehicle_routes_from_first_stable_circuit_edge(
    mut transport_vehicles: Query<
        (Entity, &CircuitMember, &mut Transform),
        (With<TransportVehicle>, Without<RoutePosition>),
    >,
    transport_circuits: Query<Option<&CircuitDirection>, With<TransportCircuit>>,
    track_segments: Query<(Entity, &TrackSegment, &CircuitMember)>,
    mut commands: Commands,
) {
    for (vehicle_entity, circuit_member, mut vehicle_transform) in &mut transport_vehicles {
        let circuit_runs_in_reverse = transport_circuits
            .get(circuit_member.0)
            .is_ok_and(|direction| matches!(direction, Some(CircuitDirection::Reverse)));
        let Some((track_segment_entity, track_segment, _)) = track_segments
            .iter()
            .filter(|(_, track_segment, track_circuit_member)| {
                track_circuit_member.0 == circuit_member.0
                    && transport_track_segment_is_valid(track_segment)
            })
            .min_by_key(|(entity, _, _)| entity.to_bits())
        else {
            continue;
        };
        let initial_distance = if circuit_runs_in_reverse {
            track_segment.length
        } else {
            0.0
        };
        let initial_position = if circuit_runs_in_reverse {
            track_segment.to_position
        } else {
            track_segment.from_position
        };
        let first_destination = if circuit_runs_in_reverse {
            track_segment.from
        } else {
            track_segment.to
        };
        vehicle_transform.translation = initial_position;
        commands.entity(vehicle_entity).insert((
            RoutePosition {
                segment: track_segment_entity,
                distance: initial_distance,
            },
            StationDestination(first_destination),
        ));
    }
}
