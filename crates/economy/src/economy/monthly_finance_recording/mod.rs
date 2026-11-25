use bevy::prelude::*;
use openzt2_game_data::world_definitions::facilities_and_maintenance::FacilityServiceKind;
use openzt2_zoo_state::economy::general_economy_types::GeneralEconomyLedger;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArrived;
use openzt2_zoo_state::guests::guest_simulation_types::GuestDeparted;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooMonth;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

use super::{
    account_transaction_settlement::RecordedAccountMovement,
    account_transaction_types::TransactionKind, facility_economy_types::ServiceFacility,
    money_types::Money, monthly_finance_types::MonthlyFinanceHistory, zoo_cash_types::ZooCash,
};

pub(super) fn enter_published_zoo_finance_month(
    mut roots: Query<
        (
            Entity,
            &ZooMonth,
            &ZooCash,
            Option<&mut GeneralEconomyLedger>,
        ),
        (With<WorldRoot>, Changed<ZooMonth>),
    >,
    mut histories: Query<&mut MonthlyFinanceHistory, With<WorldRoot>>,
) {
    for (root, month, cash, ledger) in &mut roots {
        if let Some(mut ledger) = ledger {
            ledger.enter_calendar_month(month.0);
        }
        if let Ok(mut history) = histories.get_mut(root) {
            history.enter_calendar_month(month.0, cash.0);
        }
    }
}

pub(super) fn record_completed_transactions_in_monthly_finance_history<const PLAYER: bool>(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    facilities: Query<&ServiceFacility>,
    object_definitions: Query<
        &openzt2_world_state::world_spawn::world_membership_types::DefinitionId,
    >,
    mut completed_transactions: MessageReader<RecordedAccountMovement<PLAYER>>,
    mut histories: Query<(&ZooCash, &mut MonthlyFinanceHistory), With<WorldRoot>>,
) {
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    for transaction in completed_transactions.read() {
        let zoo_cash_change = transaction.zoo_delta.0;
        let Ok((zoo_cash, mut finance_history)) = histories.get_mut(transaction.root) else {
            continue;
        };
        if !finance_history.has_initialized_month() {
            continue;
        }
        finance_history.record_completed_transaction(transaction.kind, zoo_cash_change, zoo_cash.0);
        if transaction.kind != TransactionKind::Service || zoo_cash_change <= 0 {
            continue;
        }

        if let Some((facility, name)) = transaction.named_payment {
            let category = object_definitions
                .get(facility)
                .ok()
                .and_then(|id| world_definitions?.find_object(id.0))
                .and_then(|object| object.transactions.iter().find(|entry| entry.name == name))
                .map(|entry| entry.category);
            if category
                == Some(openzt2_game_data::AssetId::from_key(
                    "concessions_food_drink",
                ))
            {
                finance_history.record_food_drink_sale(Money(zoo_cash_change));
            } else if category == Some(openzt2_game_data::AssetId::from_key("concessions_gifts")) {
                finance_history.record_gift_sale(Money(zoo_cash_change));
            }
            continue;
        }

        let Some(facility) = transaction
            .subject
            .and_then(|entity| facilities.get(entity).ok())
        else {
            continue;
        };
        let Some(facility_definition) =
            world_definitions.and_then(|view| view.find_facility(facility.definition))
        else {
            continue;
        };
        match facility_definition.service {
            FacilityServiceKind::Food | FacilityServiceKind::Drink => {
                finance_history.record_food_drink_sale(Money(zoo_cash_change));
            }
            FacilityServiceKind::Gift => finance_history.record_gift_sale(Money(zoo_cash_change)),
            _ => {}
        }
    }
}

pub(super) fn record_monthly_guest_counts(
    mut guest_arrivals: MessageReader<GuestArrived>,
    mut guest_departures: MessageReader<GuestDeparted>,
    mut histories: Query<&mut MonthlyFinanceHistory, With<WorldRoot>>,
    members: Query<&WorldMember>,
) {
    for arrival in guest_arrivals.read() {
        let Ok(member) = members.get(arrival.guest) else {
            continue;
        };
        let Ok(mut history) = histories.get_mut(member.root) else {
            continue;
        };
        if history.has_initialized_month() {
            history.record_guest_arrival();
        }
    }
    for departure in guest_departures.read() {
        let Ok(mut history) = histories.get_mut(departure.root) else {
            continue;
        };
        if history.has_initialized_month() {
            history.record_guest_departure();
        }
    }
}
