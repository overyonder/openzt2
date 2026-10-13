use bevy::{ecs::system::SystemParam, prelude::*};

use crate::plugins::{
    construction::{
        construction_interaction_types::PlacementFailure,
        construction_transaction_types::PrepareDeletion,
    },
    economy::{money_types::Money, object_sale_refund::calculate_authored_object_sale_refund},
    world_spawn::{persistent_id_types::PersistentId, world_membership_types::WorldMember},
};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::{
        WorldDefinitions, WorldDefinitionsView,
    },
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::world_definitions::world_objects::WorldObjectPropertyFlags;

use super::{
    transport_circuit_types::{CircuitMember, TransportCircuit},
    transport_topology_types::{
        TrackProfile, TrackSegment, TransportStation, TransportTrackJunction,
    },
    transport_track_construction_types::{
        PreparedTransportTrackConstructionEdit, TransportTrackConstructionEditPreparationRejected,
        TransportTrackConstructionEditPrepared, TransportTrackConstructionPiece,
    },
    transport_vehicle_types::TransportVehicle,
};

#[derive(SystemParam)]
pub(crate) struct TransportTrackSaleQueries<'w, 's> {
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
            &'static TrackSegment,
            &'static CircuitMember,
            &'static WorldMember,
        ),
    >,
    junctions:
        Query<'w, 's, (&'static PersistentId, &'static WorldMember), With<TransportTrackJunction>>,
    stations: Query<'w, 's, &'static WorldMember, With<TransportStation>>,
    circuits: Query<'w, 's, &'static WorldMember, With<TransportCircuit>>,
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

impl TransportTrackSaleQueries<'_, '_> {
    pub(crate) fn definition(&self, target: Entity) -> Option<openzt2_game_data::AssetId> {
        self.terminal_route_edit(target).map(|edit| edit.definition)
    }
    pub(crate) fn requires_transport_sale(&self, target: Entity) -> bool {
        self.transport.contains(target)
            || self
                .routes
                .iter()
                .any(|(_, route, _, _)| route.from == target || route.to == target)
    }

    // A terminal route owns its destination junction and every projected piece.
    // Shared junctions and stations need a multi-route deletion transaction.
    fn terminal_route_edit(
        &self,
        target: Entity,
    ) -> Option<PreparedTransportTrackConstructionEdit> {
        let route_entity = self
            .pieces
            .get(target)
            .ok()
            .map(|(_, _, piece, _, _)| piece.route_segment)
            .or_else(|| {
                self.junctions
                    .contains(target)
                    .then(|| {
                        self.routes
                            .iter()
                            .find(|(_, route, _, _)| route.to == target)
                            .map(|(entity, _, _, _)| entity)
                    })
                    .flatten()
            })?;
        let (_, route, member, world) = self.routes.get(route_entity).ok()?;
        let (destination_id, destination_world) = self.junctions.get(route.to).ok()?;
        if destination_world.root != world.root
            || self.circuits.get(member.0).ok()?.root != world.root
            || self.routes.iter().any(|(entity, other, _, _)| {
                entity != route_entity && (other.from == route.to || other.to == route.to)
            })
        {
            return None;
        }
        let source_is_station = self
            .stations
            .get(route.from)
            .is_ok_and(|source| source.root == world.root);
        let source_is_connected_junction = self
            .junctions
            .get(route.from)
            .is_ok_and(|(_, source)| source.root == world.root)
            && self.routes.iter().any(|(entity, other, other_member, _)| {
                entity != route_entity && other.to == route.from && other_member.0 == member.0
            });
        if !source_is_station && !source_is_connected_junction {
            return None;
        }
        let (_, _, _, profile, _) = self.pieces.get(route_entity).ok()?;
        if profile.definition != route.definition {
            return None;
        }
        let mut pieces = self
            .pieces
            .iter()
            .filter(|(_, _, piece, _, _)| piece.route_segment == route_entity)
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
        Some(PreparedTransportTrackConstructionEdit {
            removes_track: true,
            piece_entities: pieces.iter().map(|(_, id, _, _, _)| **id).collect(),
            destination_junction_entity: Some(*destination_id),
            definition: route.definition,
            kind: profile.kind,
            maximum_grade_permille: profile.maximum_grade_permille,
            circuit: member.0,
            from_endpoint: route.from,
            from_endpoint_index: route.from_endpoint_index,
            to_endpoint: None,
            to_endpoint_index: route.to_endpoint_index,
            path_points: route.path_points.clone(),
            applied_entities: pieces.iter().map(|(entity, _, _, _, _)| *entity).collect(),
            preview: None,
            application_in_flight: None,
        })
    }

    pub(crate) fn quote(
        &self,
        target: Entity,
        definitions: WorldDefinitionsView<'_>,
    ) -> Option<Money> {
        let edit = self.terminal_route_edit(target)?;
        let track = definitions.find_track(edit.definition)?;
        let object = definitions.find_object(track.object)?;
        if !object
            .properties
            .contains_all(WorldObjectPropertyFlags::DELETABLE)
        {
            return None;
        }
        let refund = calculate_authored_object_sale_refund(object)?;
        Some(Money(refund.0.checked_mul(
            i64::try_from(edit.piece_entities.len()).ok()?,
        )?))
    }

    pub(super) fn removal_still_matches(
        &self,
        edit: &PreparedTransportTrackConstructionEdit,
    ) -> bool {
        let Some(target) = self.pieces.iter().find_map(|(entity, id, _, _, _)| {
            (Some(id) == edit.piece_entities.first()).then_some(entity)
        }) else {
            return false;
        };
        self.terminal_route_edit(target).is_some_and(|current| {
            current.piece_entities == edit.piece_entities
                && current.destination_junction_entity == edit.destination_junction_entity
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

#[cfg(test)]
mod tests;

pub(crate) fn refresh_selected_transport_sale_quote(
    mut commands: Commands,
    selected: Res<crate::plugins::information::entity_selection_types::SelectedEntity>,
    sales: TransportTrackSaleQueries,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    quotes: Query<&crate::plugins::economy::facility_economy_types::CurrentSellQuote>,
) {
    use crate::plugins::economy::facility_economy_types::CurrentSellQuote;
    let Some(target) = selected
        .0
        .filter(|target| sales.requires_transport_sale(*target))
    else {
        return;
    };
    let quote = active
        .get(&definitions)
        .and_then(|definitions| sales.quote(target, definitions));
    match quote {
        Some(refund) if quotes.get(target).is_ok_and(|quote| quote.0 == refund) => {}
        Some(refund) => {
            commands.entity(target).insert(CurrentSellQuote(refund));
        }
        None => {
            commands.entity(target).remove::<CurrentSellQuote>();
        }
    }
}

pub(super) fn prepare_transport_track_sale(
    mut commands: Commands,
    mut requests: MessageReader<PrepareDeletion>,
    sales: TransportTrackSaleQueries,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    mut prepared: MessageWriter<TransportTrackConstructionEditPrepared>,
    mut rejected: MessageWriter<TransportTrackConstructionEditPreparationRejected>,
) {
    for request in requests.read() {
        if !sales.requires_transport_sale(request.target) {
            continue;
        }
        let sale = active.get(&definitions).and_then(|definitions| {
            Some((
                sales.terminal_route_edit(request.target)?,
                sales.quote(request.target, definitions)?.0.checked_neg()?,
            ))
        });
        let Some((edit, cost)) = sale else {
            rejected.write(TransportTrackConstructionEditPreparationRejected {
                transaction: request.transaction,
                reason: PlacementFailure::InvalidTopology,
            });
            continue;
        };
        commands.entity(request.transaction).insert(edit);
        prepared.write(TransportTrackConstructionEditPrepared {
            transaction: request.transaction,
            cost: Money(cost),
        });
    }
}
