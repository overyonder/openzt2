use std::collections::BTreeSet;

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::{
    economy::{facility_economy_types::Price, money_types::Money},
    transport_tours::{
        transport_topology_types::{RestoredTransportPrefab, SkyTowerHeight, TrackEnd, TrackProfile, TrackSegment, TrackStart, TransportTrackJunction},
        transport_track_construction_types::TransportTrackConstructionPiece,
    },
};
use bevy::{ecs::query::QueryData, prelude::*};
use super::restored_component_values::restore_optional_component;

use openzt2_world_state::world_spawn::persistent_id_index::PersistentIdIndex;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitMember;
use openzt2_zoo_state::transport_tours::transport_circuit_types::TransportCircuit;
use openzt2_zoo_state::transport_tours::transport_circuit_types::{
    CircuitDirection, CircuitRunning, ReopenRestoredTransportCircuit,
};
use openzt2_zoo_state::transport_tours::transport_vehicle_types::TransportVehicle;

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure,
    transport_snapshot_types::TransportSnapshotRecords,
};

pub(super) type SavedTransportEntities<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static PersistentId),
    Or<(
        With<TransportCircuit>,
        With<CircuitMember>,
        With<TransportVehicle>,
        With<TransportTrackConstructionPiece>,
    )>,
>;

pub(super) fn spawn_missing_transport_identities(
    commands: &mut Commands,
    saved: &TransportSnapshotRecords,
    definitions: WorldDefinitionsView<'_>,
    root: Entity,
    identities: &PersistentIdIndex,
    baseline: &SavedTransportEntities,
    allocator: &mut PersistentIdAllocator,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    let graph = &saved.graph;
    let transport_entities = saved
        .circuit_records
        .iter()
        .map(|record| {
            (
                record.persistent_identifier,
                record.transport_definition_identifier,
                None,
            )
        })
        .chain(saved.circuit_member_records.iter().map(|record| {
            let object = record.transport_definition_identifier;
            let model = if graph
                .junctions
                .binary_search_by_key(&record.persistent_identifier.0, |(id, _)| *id)
                .is_err()
            {
                definitions
                    .stations()
                    .find(|station| station.id == object || station.object == object)
                    .and_then(|station| definitions.find_object(station.object))
                    .and_then(|object| definitions.scene(object.prefab))
            } else {
                None
            };
            (record.persistent_identifier, object, model)
        }))
        .chain(saved.vehicle_records.iter().map(|record| {
            let object = record.transport_definition_identifier;
            let model = definitions
                .vehicles()
                .find(|vehicle| vehicle.id == object || vehicle.object == object)
                .and_then(|vehicle| definitions.find_object(vehicle.object))
                .and_then(|object| definitions.scene(object.prefab));
            (record.persistent_identifier, object, model)
        }))
        .chain(graph.routes.iter().flat_map(|route| {
            route
                .pieces
                .iter()
                .skip(1)
                .map(|id| (PersistentId(*id), route.definition, None))
        }));

    let mut saved_ids = BTreeSet::new();
    for (id, definition, model) in transport_entities {
        if !saved_ids.insert(id.0) {
            return Err(WorldSnapshotPersistenceFailure::DuplicatePersistentEntityIdentifier);
        }
        if identities.get(root, id).is_some() {
            continue;
        }
        allocator
            .reserve_imported(id)
            .map_err(|_| WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?;
        let mut entity = commands.spawn((id, DefinitionId(definition), WorldMember { root }));
        if let Some(model) = model {
            entity.insert(RestoredTransportPrefab(model));
        }
    }
    for (entity, id) in baseline {
        if identities.get(root, *id) == Some(entity) && !saved_ids.contains(&id.0) {
            commands.entity(entity).despawn();
        }
    }
    Ok(())
}

#[derive(QueryData)]
#[query_data(mutable)]
pub(super) struct RestoredTransportComponents {
    pose: Option<&'static mut Transform>,
    visibility: Option<&'static mut Visibility>,
    definition: Option<&'static mut DefinitionId>,
    circuit: Option<&'static mut TransportCircuit>,
    running: Option<&'static mut CircuitRunning>,
    direction: Option<&'static mut CircuitDirection>,
    fare: Option<&'static mut Price>,
    crossing: Option<&'static mut openzt2_transport::transport_tours::SkyStationCrossing>,
    tower: Option<&'static mut SkyTowerHeight>,
    track_profile: Option<&'static mut TrackProfile>,
    segment: Option<&'static mut TrackSegment>,
}

pub(super) fn restore_transport_sections(
    commands: &mut Commands,
    saved: TransportSnapshotRecords,
    root: Entity,
    identities: &PersistentIdIndex,
    live: &mut Query<RestoredTransportComponents>,
) -> Result<(), WorldSnapshotPersistenceFailure> {
    use WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference as Missing;
    let graph = saved.graph;
    let resolve = |id| identities.get(root, id).ok_or(Missing);
    let lookup_pose = |id: PersistentId| {
        graph
            .poses
            .binary_search_by_key(&id.0, |(id, _)| *id)
            .ok()
            .map(|index| graph.poses[index].1)
    };
    let has_saved_routes = !graph.routes.is_empty();
    for record in saved.circuit_records {
        let entity = resolve(record.persistent_identifier)?;
        let mut circuit = commands.entity(entity);
        let current = live.get_mut(entity).map_err(|_| Missing)?;
        if has_saved_routes && !record.is_closed_loop {
            circuit.insert_if_new(ReopenRestoredTransportCircuit);
        } else {
            circuit.remove::<ReopenRestoredTransportCircuit>();
        }
        let definition = record.transport_definition_identifier;
        let fare = if let Ok(index) = graph
            .fares_dollars
            .binary_search_by_key(&record.persistent_identifier.0, |(id, _)| *id)
        {
            Money::from_dollars(graph.fares_dollars[index].1).ok_or(Missing)?
        } else {
            graph
                .fares
                .binary_search_by_key(&record.persistent_identifier.0, |(id, _)| *id)
                .ok()
                .map_or(Money::ZERO, |index| Money::from_cents(graph.fares[index].1))
        };
        restore_optional_component(
            &mut circuit,
            current.definition,
            Some(DefinitionId(definition)),
            false,
        );
        restore_optional_component(
            &mut circuit,
            current.circuit,
            Some(TransportCircuit {
                definition,
                closed: has_saved_routes || record.is_closed_loop,
            }),
            false,
        );
        restore_optional_component(
            &mut circuit,
            current.running,
            Some(CircuitRunning(record.is_running)),
            false,
        );
        restore_optional_component(&mut circuit, current.fare, Some(Price(fare)), false);
        restore_optional_component(
            &mut circuit,
            current.direction,
            Some(record.travel_direction),
            false,
        );
    }

    for (id, circuit, definition) in saved
        .circuit_member_records
        .into_iter()
        .map(|record| {
            (
                record.persistent_identifier,
                record.circuit_persistent_identifier,
                record.transport_definition_identifier,
            )
        })
        .chain(saved.vehicle_records.into_iter().map(|record| {
            (
                record.persistent_identifier,
                record.circuit_persistent_identifier,
                record.transport_definition_identifier,
            )
        }))
    {
        let entity = resolve(id)?;
        let current = live.get_mut(entity).map_err(|_| Missing)?;
        let mut member = commands.entity(entity);
        if let Some(pose) = lookup_pose(id) {
            restore_optional_component(&mut member, current.pose, Some(pose), false);
            restore_optional_component(
                &mut member,
                current.visibility,
                Some(Visibility::Inherited),
                false,
            );
        }
        restore_optional_component(
            &mut member,
            current.definition,
            Some(DefinitionId(definition)),
            false,
        );
        member.insert(CircuitMember(resolve(circuit)?));
    }

    for saved in graph.sky_station_crossings {
        let vehicle = resolve(PersistentId(saved.vehicle))?;
        let route = resolve(PersistentId(saved.next_route))?;
        let current = live.get_mut(vehicle).map_err(|_| Missing)?;
        restore_optional_component(
            &mut commands.entity(vehicle),
            current.crossing,
            Some(openzt2_transport::transport_tours::SkyStationCrossing {
                next_track: route,
                reverse: saved.reverse,
                distance: saved.distance,
            }),
            false,
        );
    }

    for (id, height) in graph.junctions {
        let entity = resolve(PersistentId(id))?;
        let current = live.get_mut(entity).map_err(|_| Missing)?;
        let mut junction = commands.entity(entity);
        junction.insert_if_new(TransportTrackJunction);
        restore_optional_component(
            &mut junction,
            current.tower,
            height.map(SkyTowerHeight),
            true,
        );
    }
    for route in graph.routes {
        let first = resolve(PersistentId(route.pieces[0]))?;
        let from = resolve(PersistentId(route.from))?;
        let to = resolve(PersistentId(route.to))?;
        let profile = TrackProfile {
            definition: route.definition,
            kind: route.kind,
            maximum_grade_permille: route.maximum_grade_permille,
        };
        for (index, (&id, points)) in route.pieces.iter().zip(route.points.windows(2)).enumerate() {
            let entity = resolve(PersistentId(id))?;
            let current = live.get_mut(entity).map_err(|_| Missing)?;
            let direction = points[1] - points[0];
            let pose = Transform::from_translation(points[0].midpoint(points[1]))
                .with_rotation(Quat::from_rotation_y(direction.x.atan2(direction.z)));
            let mut piece = commands.entity(entity);
            restore_optional_component(&mut piece, current.track_profile, Some(profile), false);
            restore_optional_component(&mut piece, current.pose, Some(pose), false);
            restore_optional_component(
                &mut piece,
                current.visibility,
                Some(Visibility::Inherited),
                false,
            );
            piece.insert(TransportTrackConstructionPiece {
                route_segment: first,
                piece_index: index as u16,
            });
        }
        let current = live.get_mut(first).map_err(|_| Missing)?;
        let mut track = commands.entity(first);
        track.insert((TrackStart(from), TrackEnd(to)));
        restore_optional_component(
            &mut track,
            current.segment,
            Some(TrackSegment {
                definition: route.definition,
                from_endpoint_index: route.from_socket,
                from_position: route.points[0],
                to_endpoint_index: route.to_socket,
                to_position: route
                    .points
                    .last()
                    .copied()
                    .ok_or(WorldSnapshotPersistenceFailure::CorruptSnapshotSection)?,
                path_points: route.points.into_boxed_slice(),
                travel_path_points: route.travel_points.into_boxed_slice(),
                length: route.length,
            }),
            false,
        );
    }
    Ok(())
}
