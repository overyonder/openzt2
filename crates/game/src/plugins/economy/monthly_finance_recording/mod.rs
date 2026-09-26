use bevy::prelude::*;
use openzt2_game_data::world_definitions::facilities_and_maintenance::FacilityServiceKind;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::guests::guest_simulation_types::GuestArrived;
use crate::plugins::guests::guest_simulation_types::GuestDeparted;
use crate::plugins::simulation_time::simulation_clock_types::ZooCalendar;

use super::{
    account_transaction_types::{Account, TransactionCompleted, TransactionKind},
    facility_economy_types::ServiceFacility,
    money_types::Money,
    monthly_finance_types::MonthlyFinanceHistory,
    zoo_cash_types::ZooCash,
};

pub(super) fn record_completed_transactions_and_guest_counts_in_monthly_finance_history(
    calendar: Res<ZooCalendar>,
    zoo_cash: Res<ZooCash>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    facilities: Query<&ServiceFacility>,
    named_payments: Query<&super::behavior_transaction_state::BehaviorTransactionPayment>,
    object_definitions: Query<&crate::plugins::world_spawn::world_membership_types::DefinitionId>,
    mut completed_transactions: MessageReader<TransactionCompleted>,
    mut guest_arrivals: MessageReader<GuestArrived>,
    mut guest_departures: MessageReader<GuestDeparted>,
    mut finance_history: ResMut<MonthlyFinanceHistory>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let calendar_month_ordinal = u32::from(calendar.year)
        .saturating_mul(12)
        .saturating_add(u32::from(calendar.month.saturating_sub(1)));
    finance_history.enter_calendar_month(calendar_month_ordinal, zoo_cash.0);

    for transaction in completed_transactions.read() {
        let zoo_cash_change = match (transaction.debit, transaction.credit) {
            (Account::Zoo, Account::Zoo) => 0,
            (Account::Zoo, _) => -transaction.amount.0,
            (_, Account::Zoo) => transaction.amount.0,
            _ => continue,
        };
        finance_history.record_completed_transaction(transaction.kind, zoo_cash_change, zoo_cash.0);
        if transaction.kind != TransactionKind::Service || zoo_cash_change <= 0 {
            continue;
        }

        if let Ok(payment) = named_payments.get(transaction.operation) {
            let category = object_definitions
                .get(payment.facility)
                .ok()
                .and_then(|id| world_definitions.find_object(id.0))
                .and_then(|object| {
                    object
                        .transactions
                        .iter()
                        .find(|entry| entry.name == payment.transaction)
                })
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
        let Some(facility_definition) = world_definitions.find_facility(facility.definition) else {
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

    guest_arrivals
        .read()
        .for_each(|_| finance_history.record_guest_arrival());
    guest_departures
        .read()
        .for_each(|_| finance_history.record_guest_departure());
}
