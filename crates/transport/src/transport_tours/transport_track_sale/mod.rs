use bevy::prelude::*;
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
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

pub(crate) fn refresh_selected_transport_sale_quote(
    mut commands: Commands,
    selected: Res<openzt2_interface_state::information::entity_selection_types::SelectedEntity>,
    sales: TransportTrackSaleQueries,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    quotes: Query<&openzt2_zoo_state::economy::facility_economy_types::CurrentSellQuote>,
) {
    use openzt2_zoo_state::economy::facility_economy_types::CurrentSellQuote;
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
    mut allocator: ResMut<PersistentIdAllocator>,
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
        if let Some((index, world)) = sales.single_piece(request.target) {
            let Some(steps) = single_piece_removal_steps(
                &mut commands,
                &mut allocator,
                request.transaction,
                &edit,
                index,
                world,
            ) else {
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
    let mut allocate = || allocator.allocate(world.root).ok();
    let laid = |points: &[bevy::math::Vec3]| PreparedTransportTrackConstructionEdit {
        removes_track: false,
        piece_entities: Box::default(),
        destination_junction_entity: None,
        source_junction_entity: None,
        path_points: points.into(),
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
    let steps = edits
        .into_iter()
        .map(|edit| commands.spawn((edit, world, ChildOf(transaction))).id())
        .collect();
    Some(TransportTrackConstructionEditSteps {
        steps,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests;
