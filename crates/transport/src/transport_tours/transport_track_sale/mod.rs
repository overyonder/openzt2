use bevy::prelude::*;
use super::transport_track_sale_steps::{TrackSaleStepState, TransportTrackSaleStep};
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareDeletion;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditPreparationRejected;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditPrepared;
use openzt2_zoo_state::transport_tours::transport_track_sale::TransportTrackSaleQueries;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::PreparedTransportTrackConstructionEdit;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditSteps;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::{TrackPieces, TransportTrackConstructionPiece};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_interface_state::information::entity_selection_types::SelectedSubject;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::InGameUiOwner;
use openzt2_game_data::world_definitions::world_objects::WorldObjectPropertyFlags;
use openzt2_zoo_state::economy::object_sale_refund::calculate_authored_object_sale_refund;
use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitMember;
use openzt2_zoo_state::transport_tours::transport_topology_types::{
    TrackEnd, TrackIncidence, TrackProfile, TrackRoute, TrackSegment, TrackStart,
};

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct TransportSaleQuoteChanges<'w, 's> {
    pieces: Query<'w, 's, Ref<'static, TransportTrackConstructionPiece>>,
    changed_piece_profiles:
        Query<'w, 's, &'static TransportTrackConstructionPiece, Changed<TrackProfile>>,
    routes: Query<'w, 's, TrackRoute>,
    changed_routes: Query<
        'w,
        's,
        (),
        Or<(
            Changed<TrackSegment>,
            Changed<TrackPieces>,
            Changed<CircuitMember>,
            Changed<TrackStart>,
            Changed<TrackEnd>,
            Changed<WorldMember>,
        )>,
    >,
    incidence: TrackIncidence<'w, 's>,
    discarded: MessageReader<
        'w,
        's,
        super::transport_track_piece_presentation::DiscardedTrackJointEndpoints,
    >,
}

pub(crate) fn refresh_selected_transport_sale_quote(
    mut commands: Commands,
    selected: Query<Ref<SelectedSubject>, With<InGameUiOwner>>,
    sales: TransportTrackSaleQueries,
    mut changes: TransportSaleQuoteChanges,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    mut quotes: Query<&mut openzt2_zoo_state::economy::facility_economy_types::CurrentSellQuote>,
) {
    use openzt2_zoo_state::economy::facility_economy_types::CurrentSellQuote;
    let Ok(selected) = selected.single() else {
        changes.discarded.clear();
        return;
    };
    let target = selected.0;
    let piece = changes.pieces.get(target).ok();
    let route = piece.as_ref().map(|piece| piece.route_segment).or_else(|| {
        changes
            .incidence
            .incoming(target)
            .find(|route| changes.routes.contains(*route))
    });
    let destination =
        route.and_then(|route| changes.routes.get(route).ok().map(|route| route.to()));
    let joint_discarded = changes.discarded.read().any(|event| {
        event.0.contains(&target) || destination.is_some_and(|end| event.0.contains(&end))
    });
    changes.discarded.clear();
    let topology_changed = joint_discarded
        || piece.as_ref().is_some_and(|piece| piece.is_changed())
        || route.is_some_and(|route| {
            changes.changed_routes.contains(route)
                || changes
                    .changed_piece_profiles
                    .iter()
                    .any(|piece| piece.route_segment == route)
        })
        || destination.is_some_and(|end| {
            changes
                .incidence
                .incident(end)
                .any(|route| changes.changed_routes.contains(route))
        });
    if !selected.is_changed()
        && !active.is_changed()
        && !definitions.is_changed()
        && !topology_changed
    {
        return;
    }
    if !sales.requires_transport_sale(target) && !joint_discarded {
        return;
    }
    let quote = active
        .get(&definitions)
        .and_then(|definitions| sales.quote(target, definitions));
    match (quote, quotes.get_mut(target)) {
        (Some(refund), Ok(mut current)) => {
            current.set_if_neq(CurrentSellQuote(refund));
        }
        (Some(refund), Err(_)) => {
            commands.entity(target).insert(CurrentSellQuote(refund));
        }
        (None, Ok(_)) => {
            commands.entity(target).remove::<CurrentSellQuote>();
        }
        (None, Err(_)) => {}
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
    mut allocators: Query<&mut PersistentIdAllocator>,
) {
    for request in requests.read() {
        if !sales.requires_transport_sale(request.target) {
            continue;
        }
        let piece = sales.single_piece(request.target);
        let sale = active.get(&definitions).and_then(|definitions| {
            let edit = sales.terminal_route_edit(request.target)?;
            let removed = if piece.is_some() {
                1
            } else {
                edit.piece_entities.len()
            };
            let definition = definitions.find_track(edit.definition)?;
            let object = definitions.find_object(definition.object)?;
            if !object
                .properties
                .contains_all(WorldObjectPropertyFlags::DELETABLE)
            {
                return None;
            }
            let refund = calculate_authored_object_sale_refund(object)?;
            let cost = refund
                .0
                .checked_mul(i64::try_from(removed).ok()?)?
                .checked_neg()?;
            Some((edit, cost))
        });
        let Some((edit, cost)) = sale else {
            rejected.write(TransportTrackConstructionEditPreparationRejected {
                transaction: request.transaction,
                reason: PlacementFailure::InvalidTopology,
            });
            continue;
        };
        if let Some((index, world)) = piece {
            let steps = allocators
                .get_mut(world.root)
                .ok()
                .and_then(|mut allocator| {
                    single_piece_removal_steps(
                        &mut commands,
                        &mut allocator,
                        request.transaction,
                        &edit,
                        index,
                        world,
                    )
                });
            let Some(steps) = steps else {
                rejected.write(TransportTrackConstructionEditPreparationRejected {
                    transaction: request.transaction,
                    reason: PlacementFailure::InvalidTopology,
                });
                continue;
            };
            commands.entity(request.transaction).insert(steps);
        }
        commands.entity(request.transaction).insert(edit);
        prepared.write(TransportTrackConstructionEditPrepared {
            transaction: request.transaction,
            cost: Money(cost),
        });
    }
}

/// Taking piece `index` out of a route: remove the route, then lay the road
/// before the piece to a new junction and the road after it from another,
/// as the original leaves a gap one piece long.
fn single_piece_removal_steps(
    commands: &mut Commands,
    allocator: &mut PersistentIdAllocator,
    transaction: Entity,
    removal: &PreparedTransportTrackConstructionEdit,
    index: usize,
    world: WorldMember,
) -> Option<TransportTrackConstructionEditSteps> {
    let mut allocate = || allocator.allocate().ok();
    let laid = |points: &[bevy::math::Vec3]| PreparedTransportTrackConstructionEdit {
        removes_track: false,
        piece_entities: Box::default(),
        destination_junction_entity: None,
        source_junction_entity: None,
        path_points: points.into(),
        tower_height_m: None,
        applied_entities: Vec::new(),
        preview: None,
        application_in_flight: None,
        to_endpoint: None,
        to_endpoint_index: 0,
        ..removal.clone()
    };
    let mut edits = vec![removal.clone()];
    let points = &removal.path_points;
    if index > 0 {
        let mut before = laid(points.get(..=index)?);
        before.piece_entities = (0..index).map(|_| allocate()).collect::<Option<_>>()?;
        before.destination_junction_entity = Some(allocate()?);
        edits.push(before);
    }
    if index + 2 < points.len() {
        let mut after = laid(points.get(index + 1..)?);
        after.piece_entities = (index + 1..points.len() - 1)
            .map(|_| allocate())
            .collect::<Option<_>>()?;
        after.source_junction_entity = Some(allocate()?);
        after.from_endpoint_index = 0;
        // The road beyond keeps its end: a junction the route owned is laid
        // again, anything else is met where it stands.
        if removal.destination_junction_entity.is_some() {
            after.destination_junction_entity = Some(allocate()?);
        } else {
            after.to_endpoint = removal.to_endpoint;
            after.to_endpoint_index = removal.to_endpoint_index;
        }
        edits.push(after);
    }
    for (ordinal, edit) in edits.into_iter().enumerate() {
        commands.spawn((
            edit,
            world,
            ChildOf(transaction),
            TransportTrackSaleStep {
                ordinal,
                state: TrackSaleStepState::Queued,
            },
        ));
    }
    Some(TransportTrackConstructionEditSteps::default())
}

#[cfg(test)]
mod tests;
