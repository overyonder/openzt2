use std::ops::Range;

use bevy::{
    ecs::{entity::EntityHashSet, system::SystemParam},
    prelude::*,
};
use openzt2_world_state::world_spawn::{
    persistent_id_index::PersistentIdIndex,
    persistent_id_types::{PersistentId, PersistentIdAllocator},
    world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::{
    construction::construction_transaction_types::EditApplication,
    transport_tours::{
        transport_circuit_types::{CircuitMember, CircuitMembers, TransportCircuit},
        transport_topology_types::{
            TrackEnd, TrackIncidence, TrackProfile, TrackRoute, TrackSegment, TrackStart,
            TransportTrackJunction,
        },
        transport_track_construction_types::{
            ApplyPreparedTransportTrackConstructionEditRequest,
            PreparedTransportTrackConstructionEdit,
            TransportTrackConstructionEditApplicationAcknowledged, TransportTrackConstructionPiece,
        },
        transport_vehicle_types::{RoutePosition, VehicleOnTrack, VehiclesOnTrack},
    },
};

use super::{
    transport_route_position_calculations::{
        construct_native_ground_transport_track_travel_path_points, TrackTravelDistances,
    },
    transport_track_construction_transaction::transport_track_piece_transform,
};

#[derive(Component)]
pub(super) struct PreparedRoadCut {
    sold_piece: usize,
    left_end: Option<PersistentId>,
    right_start: Option<PersistentId>,
    cut_applied: bool,
}

impl PreparedRoadCut {
    pub(super) fn new(index: usize, count: usize, ids: &mut PersistentIdAllocator) -> Option<Self> {
        if count < 2 || index >= count {
            return None;
        }
        Some(Self {
            sold_piece: index,
            left_end: if index > 0 {
                Some(ids.allocate().ok()?)
            } else {
                None
            },
            right_start: if index + 1 < count {
                Some(ids.allocate().ok()?)
            } else {
                None
            },
            cut_applied: false,
        })
    }
}

struct RoadRun {
    pieces: Range<usize>,
    segment: TrackSegment,
    distances: TrackTravelDistances,
}

impl RoadRun {
    fn new(
        edit: &PreparedTransportTrackConstructionEdit,
        pieces: Range<usize>,
        from_index: u8,
        to_index: u8,
    ) -> Option<Self> {
        let points = edit.path_points.get(pieces.start..=pieces.end)?;
        let travel_path_points = construct_native_ground_transport_track_travel_path_points(points);
        let (distances, length) = TrackTravelDistances::calculate(&travel_path_points)?;
        Some(Self {
            segment: TrackSegment {
                definition: edit.definition,
                from_endpoint_index: from_index,
                from_position: *points.first()?,
                to_endpoint_index: to_index,
                to_position: *points.last()?,
                path_points: points.into(),
                travel_path_points,
                length,
            },
            distances,
            pieces,
        })
    }
}

#[derive(SystemParam)]
pub(super) struct RoadFacts<'w, 's> {
    pieces: Query<'w, 's, &'static TransportTrackConstructionPiece>,
    routes: Query<'w, 's, TrackRoute>,
    worlds: Query<'w, 's, &'static WorldMember>,
    circuits: Query<'w, 's, &'static CircuitMembers>,
    members: Query<'w, 's, &'static CircuitMember>,
    incidence: TrackIncidence<'w, 's>,
    vehicles: Query<'w, 's, &'static VehiclesOnTrack>,
}

impl RoadFacts<'_, '_> {
    fn run_matches(
        &self,
        run: &RoadRun,
        pieces: &[Option<Entity>],
        from: Entity,
        to: Entity,
        world: WorldMember,
    ) -> bool {
        let Some(root) = pieces[run.pieces.start] else {
            return false;
        };
        self.routes.get(root).is_ok_and(|route| {
            route.from() == from
                && route.to() == to
                && route.path_points == run.segment.path_points
                && route.from_endpoint_index == run.segment.from_endpoint_index
                && route.to_endpoint_index == run.segment.to_endpoint_index
        }) && pieces[run.pieces.clone()]
            .iter()
            .enumerate()
            .all(|(index, entity)| {
                entity.is_some_and(|entity| {
                    self.worlds
                        .get(entity)
                        .is_ok_and(|member| member.root == world.root)
                        && self.pieces.get(entity).is_ok_and(|piece| {
                            piece.route_segment == root && usize::from(piece.piece_index) == index
                        })
                })
            })
    }

    fn clear_vehicle_routes(&self, commands: &mut Commands, route: Entity) {
        if let Ok(vehicles) = self.vehicles.get(route) {
            for vehicle in vehicles.iter() {
                commands.entity(vehicle).try_remove::<(
                    VehicleOnTrack,
                    RoutePosition,
                    super::transport_vehicle_movement::SkyStationCrossing,
                )>();
            }
        }
    }
}

#[derive(SystemParam)]
pub(super) struct RoadWrites<'w, 's> {
    runs: Query<
        'w,
        's,
        (
            &'static mut TrackSegment,
            Option<&'static mut TrackTravelDistances>,
        ),
    >,
    pieces: Query<'w, 's, &'static mut TransportTrackConstructionPiece>,
    sockets: Query<'w, 's, (Option<&'static TrackStart>, Option<&'static TrackEnd>)>,
    members: Query<'w, 's, &'static CircuitMember>,
}

impl RoadWrites<'_, '_> {
    fn bind_run(
        &mut self,
        commands: &mut Commands,
        run: RoadRun,
        pieces: &[Option<Entity>],
        circuit: Entity,
        from: Entity,
        to: Entity,
    ) {
        let root = pieces[run.pieces.start].expect("validated first road piece");
        let mut entity = commands.entity(root);
        let (start, end) = self.sockets.get(root).unwrap_or_default();
        if start != Some(&TrackStart(from)) {
            entity.insert(TrackStart(from));
        }
        if end != Some(&TrackEnd(to)) {
            entity.insert(TrackEnd(to));
        }
        if !self
            .members
            .get(root)
            .is_ok_and(|member| member.0 == circuit)
        {
            entity.insert(CircuitMember(circuit));
        }
        if let Ok((mut segment, distances)) = self.runs.get_mut(root) {
            segment.set_if_neq(run.segment);
            if let Some(mut distances) = distances {
                distances.set_if_neq(run.distances);
            } else {
                entity.insert(run.distances);
            }
        } else {
            entity.insert((run.segment, run.distances));
        }
        for (index, piece) in pieces[run.pieces].iter().copied().enumerate() {
            let piece = piece.expect("validated road piece");
            let piece_index = u16::try_from(index).expect("existing road piece index fits");
            match self.pieces.get_mut(piece) {
                Ok(mut current) if current.route_segment == root => {
                    current
                        .map_unchanged(|piece| &mut piece.piece_index)
                        .set_if_neq(piece_index);
                }
                _ => {
                    commands
                        .entity(piece)
                        .insert(TransportTrackConstructionPiece {
                            route_segment: root,
                            piece_index,
                        });
                }
            }
        }
    }
}

fn spawn_gap_endpoint(
    commands: &mut Commands,
    edit: &PreparedTransportTrackConstructionEdit,
    world: WorldMember,
    circuit: Entity,
    id: PersistentId,
    position: Vec3,
) -> Entity {
    commands
        .spawn((
            id,
            DefinitionId(edit.definition),
            TransportTrackJunction,
            CircuitMember(circuit),
            Transform::from_translation(position),
            GlobalTransform::from_translation(position),
            Visibility::Inherited,
            world,
        ))
        .id()
}

pub(super) fn apply_prepared_road_cuts(
    mut requests: MessageReader<ApplyPreparedTransportTrackConstructionEditRequest>,
    mut cuts: Query<(
        &mut PreparedRoadCut,
        &PreparedTransportTrackConstructionEdit,
        &WorldMember,
    )>,
    ids: Res<PersistentIdIndex>,
    mut road: ParamSet<(RoadFacts, RoadWrites)>,
    mut circuits: Query<&mut TransportCircuit>,
    mut commands: Commands,
    mut results: MessageWriter<TransportTrackConstructionEditApplicationAcknowledged>,
    mut split: MessageWriter<super::topology::TransportEdgesDisconnected>,
) {
    let mut reserved = EntityHashSet::default();
    for request in requests.read() {
        let Ok((mut cut, edit, world)) = cuts.get_mut(request.transaction) else {
            continue;
        };
        let sell = match request.application {
            EditApplication::InitialCommit | EditApplication::Redo => true,
            EditApplication::Undo => false,
            EditApplication::FailureRollback => !cut.cut_applied,
        };
        let accepted = (|| {
            if sell == cut.cut_applied {
                return Some(());
            }
            let count = edit.piece_entities.len();
            let mut pieces = edit
                .piece_entities
                .iter()
                .map(|&id| ids.get(world.root, id))
                .collect::<Vec<_>>();
            if pieces
                .iter()
                .flatten()
                .any(|piece| reserved.contains(piece))
            {
                return None;
            }
            let whole = RoadRun::new(
                edit,
                0..count,
                edit.from_endpoint_index,
                edit.to_endpoint_index,
            )?;
            let left = if cut.left_end.is_some() {
                Some(RoadRun::new(
                    edit,
                    0..cut.sold_piece,
                    edit.from_endpoint_index,
                    0,
                )?)
            } else {
                None
            };
            let right = if cut.right_start.is_some() {
                Some(RoadRun::new(
                    edit,
                    cut.sold_piece + 1..count,
                    0,
                    edit.to_endpoint_index,
                )?)
            } else {
                None
            };
            let from = edit.from_endpoint;
            let to = edit
                .to_endpoint
                .or_else(|| ids.get(world.root, edit.destination_junction_entity?))?;
            let left_end = cut.left_end.and_then(|id| ids.get(world.root, id));
            let right_start = cut.right_start.and_then(|id| ids.get(world.root, id));
            let facts = road.p0();
            let checked_world = |entity| {
                facts
                    .worlds
                    .get(entity)
                    .is_ok_and(|member| member.root == world.root)
            };
            if !checked_world(from) || !checked_world(to) {
                return None;
            }
            let source_circuit = facts.members.get(from).ok().map(|member| member.0);
            let destination_circuit = facts.members.get(to).ok().map(|member| member.0);
            let circuit = source_circuit
                .or(destination_circuit)
                .unwrap_or(edit.circuit);
            let merge = destination_circuit.filter(|&other| other != circuit);
            if sell {
                if left_end.is_some()
                    || right_start.is_some()
                    || !facts.run_matches(&whole, &pieces, from, to, *world)
                {
                    return None;
                }
            } else {
                if pieces[cut.sold_piece].is_some() {
                    return None;
                }
                for (run, junction, from, to) in [
                    (left.as_ref(), left_end, Some(from), left_end),
                    (right.as_ref(), right_start, right_start, Some(to)),
                ] {
                    if let Some(run) = run {
                        let root = pieces[run.pieces.start]?;
                        let junction = junction?;
                        if !checked_world(junction)
                            || !facts.run_matches(run, &pieces, from?, to?, *world)
                            || facts
                                .incidence
                                .incident(junction)
                                .any(|track| track != root)
                        {
                            return None;
                        }
                    }
                }
                for (socket, index, run, outgoing) in [
                    (from, edit.from_endpoint_index, left.as_ref(), true),
                    (to, edit.to_endpoint_index, right.as_ref(), false),
                ] {
                    let kept = run.and_then(|run| pieces[run.pieces.start]);
                    if facts.incidence.incident(socket).any(|entity| {
                        Some(entity) != kept
                            && facts.routes.get(entity).is_ok_and(|route| {
                                if outgoing {
                                    route.from() == socket && route.from_endpoint_index == index
                                } else {
                                    route.to() == socket && route.to_endpoint_index == index
                                }
                            })
                    }) {
                        return None;
                    }
                }
            }
            let mut circuit_state = circuits.get_mut(circuit).ok()?;
            reserved.extend(pieces.iter().flatten().copied());
            circuit_state.closed = true;
            if !sell {
                if let Some(other) = merge {
                    if let Ok(members) = facts.circuits.get(other) {
                        for member in members.iter() {
                            commands.entity(member).insert(CircuitMember(circuit));
                        }
                    }
                    commands.entity(other).despawn();
                }
                for endpoint in [from, to] {
                    if !facts.members.contains(endpoint) {
                        commands.entity(endpoint).insert(CircuitMember(circuit));
                    }
                }
            }
            if sell {
                split.write(super::topology::TransportEdgesDisconnected(circuit));
                facts.clear_vehicle_routes(&mut commands, pieces[0]?);
            } else {
                for run in [left.as_ref(), right.as_ref()].into_iter().flatten() {
                    facts.clear_vehicle_routes(&mut commands, pieces[run.pieces.start]?);
                }
            }
            let mut writes = road.p1();
            if sell {
                if let Some((run, id)) = left.zip(cut.left_end) {
                    let end = spawn_gap_endpoint(
                        &mut commands,
                        edit,
                        *world,
                        circuit,
                        id,
                        run.segment.to_position,
                    );
                    writes.bind_run(&mut commands, run, &pieces, circuit, from, end);
                }
                if let Some((run, id)) = right.zip(cut.right_start) {
                    let start = spawn_gap_endpoint(
                        &mut commands,
                        edit,
                        *world,
                        circuit,
                        id,
                        run.segment.from_position,
                    );
                    writes.bind_run(&mut commands, run, &pieces, circuit, start, to);
                }
                commands.entity(pieces[cut.sold_piece]?).despawn();
            } else {
                let [start, end] = [
                    edit.path_points[cut.sold_piece],
                    edit.path_points[cut.sold_piece + 1],
                ];
                let restored = commands
                    .spawn((
                        edit.piece_entities[cut.sold_piece],
                        DefinitionId(edit.definition),
                        TrackProfile {
                            definition: edit.definition,
                            kind: edit.kind,
                            maximum_grade_permille: edit.maximum_grade_permille,
                        },
                        transport_track_piece_transform(start, end),
                        Visibility::Inherited,
                        *world,
                    ))
                    .id();
                pieces[cut.sold_piece] = Some(restored);
                if let Some(root) = right.as_ref().and_then(|run| pieces[run.pieces.start]) {
                    commands.entity(root).remove::<(
                        TrackStart,
                        TrackEnd,
                        TrackSegment,
                        TrackTravelDistances,
                        CircuitMember,
                    )>();
                }
                writes.bind_run(&mut commands, whole, &pieces, circuit, from, to);
                for junction in [left_end, right_start].into_iter().flatten() {
                    commands.entity(junction).despawn();
                }
            }
            cut.cut_applied = sell;
            Some(())
        })()
        .is_some();
        results.write(TransportTrackConstructionEditApplicationAcknowledged {
            transaction: request.transaction,
            application: request.application,
            accepted,
        });
    }
}
