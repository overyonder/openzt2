use bevy::prelude::*;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareDeletion;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditPreparationRejected;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditPrepared;
use openzt2_zoo_state::transport_tours::transport_track_sale::TransportTrackSaleQueries;

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

#[cfg(test)]
mod tests;
