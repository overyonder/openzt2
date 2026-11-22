use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_world_state::world_spawn::persistent_id_index::PersistentIdIndex;

use crate::economy::money_types::Money;
use crate::economy::object_sale_refund::calculate_authored_object_sale_refund;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::world_definitions::world_objects::WorldObjectPropertyFlags;

use crate::transport_tours::transport_circuit_types::CircuitMember;
use crate::transport_tours::transport_circuit_types::TransportCircuit;
use crate::transport_tours::transport_topology_types::TrackProfile;
use crate::transport_tours::transport_topology_types::TransportStation;
use crate::transport_tours::transport_topology_types::TransportTrackJunction;
use crate::transport_tours::transport_topology_types::{TrackIncidence, TrackRoute, TrackSegment};
use crate::transport_tours::transport_track_construction_types::PreparedTransportTrackConstructionEdit;
use crate::transport_tours::transport_track_construction_types::{
    TrackPieces, TransportTrackConstructionPiece,
};
use crate::transport_tours::transport_vehicle_types::TransportVehicle;

#[derive(SystemParam)]
pub struct TransportTrackSaleQueries<'w, 's> {
    pieces: Query<
        'w,
        's,
        (
            Entity,
            &'static PersistentId,
            &'static TransportTrackConstructionPiece,
            &'static TrackProfile,
            &'static WorldMember,
        ),
    >,
    routes: Query<
        'w,
        's,
        (
            Entity,
            TrackRoute,
            &'static CircuitMember,
            &'static WorldMember,
        ),
    >,
    junctions:
        Query<'w, 's, (&'static PersistentId, &'static WorldMember), With<TransportTrackJunction>>,
    tower_heights: Query<'w, 's, &'static super::transport_topology_types::SkyTowerHeight>,
    stations: Query<'w, 's, &'static WorldMember, With<TransportStation>>,
    circuits: Query<'w, 's, &'static WorldMember, With<TransportCircuit>>,
    incidence: TrackIncidence<'w, 's>,
    route_pieces: Query<'w, 's, &'static TrackPieces>,
    identities: Res<'w, PersistentIdIndex>,
    transport: Query<
        'w,
        's,
        (),
        Or<(
            With<TrackProfile>,
            With<TransportTrackJunction>,
            With<TransportVehicle>,
            With<TransportCircuit>,
        )>,
    >,
}

struct TerminalTrackSale<'a> {
    route: &'a TrackSegment,
    from: Entity,
    to: Entity,
    profile: &'a TrackProfile,
    circuit: Entity,
    destination_junction: Option<PersistentId>,
    destination_endpoint: Option<Entity>,
    pieces: Vec<(Entity, PersistentId)>,
}

impl TransportTrackSaleQueries<'_, '_> {
    pub fn definition(&self, target: Entity) -> Option<openzt2_game_data::AssetId> {
        self.terminal_route_sale(target)
            .map(|sale| sale.route.definition)
    }
    pub fn requires_transport_sale(&self, target: Entity) -> bool {
        self.transport.contains(target)
            || self
                .incidence
                .incident(target)
                .any(|route| self.routes.contains(route))
    }

    // A terminal route owns its destination junction and every projected piece.
    // A route with more road beyond it keeps its destination, so removing it
    // leaves a gap with a free road end on each side, as in the original.
    fn terminal_route_sale(&self, target: Entity) -> Option<TerminalTrackSale<'_>> {
        let route_entity = self
            .pieces
            .get(target)
            .ok()
            .map(|(_, _, piece, _, _)| piece.route_segment)
            .or_else(|| {
                self.junctions
                    .contains(target)
                    .then(|| {
                        self.incidence
                            .incoming(target)
                            .find(|&route| self.routes.contains(route))
                    })
                    .flatten()
            })?;
        let (_, route, member, world) = self.routes.get(route_entity).ok()?;
        let destination =
            if let Ok((destination_id, destination_world)) = self.junctions.get(route.to()) {
                (destination_world.root == world.root).then_some(())?;
                let continues = self
                    .incidence
                    .outgoing(route.to())
                    .any(|entity| entity != route_entity && self.routes.contains(entity));
                if self
                    .incidence
                    .incoming(route.to())
                    .any(|entity| entity != route_entity && self.routes.contains(entity))
                {
                    return None;
                }
                if continues {
                    (None, Some(route.to()))
                } else {
                    (Some(*destination_id), None)
                }
            } else {
                (self.stations.get(route.to()).ok()?.root == world.root).then_some(())?;
                (None, Some(route.to()))
            };
        if self.circuits.get(member.0).ok()?.root != world.root {
            return None;
        }
        let source_is_station = self
            .stations
            .get(route.from())
            .is_ok_and(|source| source.root == world.root);
        // A road left after a gap can be removed from its first
        // piece too; undoing that is refused, since nothing leads into it.
        let source_is_junction = self
            .junctions
            .get(route.from())
            .is_ok_and(|(_, source)| source.root == world.root);
        if !source_is_station && !source_is_junction {
            return None;
        }
        let (_, _, _, profile, _) = self.pieces.get(route_entity).ok()?;
        if profile.definition != route.definition {
            return None;
        }
        let mut pieces = self
            .pieces
            .iter_many(self.route_pieces.get(route_entity).ok()?.iter())
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        pieces.sort_unstable_by_key(|(_, _, piece, _, _)| piece.piece_index);
        if pieces.is_empty()
            || pieces.len().checked_add(1)? != route.path_points.len()
            || pieces[0].0 != route_entity
            || pieces.iter().enumerate().any(
                |(index, (_, _, piece, piece_profile, piece_world))| {
                    usize::from(piece.piece_index) != index
                        || *piece_profile != profile
                        || piece_world.root != world.root
                },
            )
        {
            return None;
        }
        Some(TerminalTrackSale {
            route: route.geometry,
            from: route.from(),
            to: route.to(),
            profile,
            circuit: member.0,
            destination_junction: destination.0,
            destination_endpoint: destination.1,
            pieces: pieces
                .into_iter()
                .map(|(entity, id, ..)| (entity, *id))
                .collect(),
        })
    }

    pub fn can_sell_track_at(&self, target: Entity) -> bool {
        self.terminal_route_sale(target).is_some()
    }

    pub fn terminal_route_edit(
        &self,
        target: Entity,
    ) -> Option<PreparedTransportTrackConstructionEdit> {
        let sale = self.terminal_route_sale(target)?;
        let route = sale.route;
        Some(PreparedTransportTrackConstructionEdit {
            removes_track: true,
            piece_entities: sale.pieces.iter().map(|(_, id)| *id).collect(),
            destination_junction_entity: sale.destination_junction,
            source_junction_entity: None,
            definition: route.definition,
            kind: sale.profile.kind,
            maximum_grade_permille: sale.profile.maximum_grade_permille,
            circuit: sale.circuit,
            from_endpoint: sale.from,
            from_endpoint_index: route.from_endpoint_index,
            to_endpoint: sale.destination_endpoint,
            to_endpoint_index: route.to_endpoint_index,
            path_points: route.path_points.clone(),
            tower_height_m: self.tower_heights.get(sale.to).ok().map(|height| height.0),
            applied_entities: sale.pieces.into_iter().map(|(entity, _)| entity).collect(),
            preview: None,
            application_in_flight: None,
        })
    }

    /// A road piece's place in its route and its world, when the delete tool
    /// takes that one piece out of a longer route.
    #[must_use]
    pub fn single_piece(&self, target: Entity) -> Option<(usize, WorldMember)> {
        let (_, _, piece, _, world) = self.pieces.get(target).ok()?;
        let (_, route, _, _) = self.routes.get(piece.route_segment).ok()?;
        (route.path_points.len() > 2).then_some((usize::from(piece.piece_index), *world))
    }

    pub fn quote(&self, target: Entity, definitions: WorldDefinitionsView<'_>) -> Option<Money> {
        let sale = self.terminal_route_sale(target)?;
        let removed_pieces = if self.single_piece(target).is_some() {
            1
        } else {
            sale.pieces.len()
        };
        let track = definitions.find_track(sale.route.definition)?;
        let object = definitions.find_object(track.object)?;
        if !object
            .properties
            .contains_all(WorldObjectPropertyFlags::DELETABLE)
        {
            return None;
        }
        let refund = calculate_authored_object_sale_refund(object)?;
        Some(Money(
            refund.0.checked_mul(i64::try_from(removed_pieces).ok()?)?,
        ))
    }

    pub fn removal_still_matches(&self, edit: &PreparedTransportTrackConstructionEdit) -> bool {
        let target = self.circuits.get(edit.circuit).ok().and_then(|world| {
            self.identities
                .get(world.root, *edit.piece_entities.first()?)
        });
        let Some(target) = target else { return false };
        self.terminal_route_edit(target).is_some_and(|current| {
            current.piece_entities == edit.piece_entities
                && current.destination_junction_entity == edit.destination_junction_entity
                && current.to_endpoint == edit.to_endpoint
                && current.definition == edit.definition
                && current.kind == edit.kind
                && current.maximum_grade_permille == edit.maximum_grade_permille
                && current.circuit == edit.circuit
                && current.from_endpoint == edit.from_endpoint
                && current.from_endpoint_index == edit.from_endpoint_index
                && current.to_endpoint_index == edit.to_endpoint_index
                && current.path_points == edit.path_points
        })
    }
}
