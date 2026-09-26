use bevy::prelude::*;

use crate::plugins::simulation_time::simulation_clock_types::ZooClock;

use super::{
    account_transaction_types::{Account, TransactionCompleted},
    facility_economy_types::{FacilityProfit, OperatingSinceDay, ServiceFacility},
    money_types::Money,
};

pub(super) fn initialize_new_service_facilities_with_operating_age_and_profit(
    mut commands: Commands,
    clock: Res<ZooClock>,
    facilities: Query<
        (Entity, Option<&FacilityProfit>),
        (With<ServiceFacility>, Without<OperatingSinceDay>),
    >,
) {
    for (facility, profit) in &facilities {
        let mut entity = commands.entity(facility);
        entity.insert(OperatingSinceDay(clock.absolute_day));
        if profit.is_none() {
            entity.insert(FacilityProfit::default());
        }
    }
}

pub(super) fn record_completed_transaction_results_for_attributed_facilities(
    mut completed_transactions: MessageReader<TransactionCompleted>,
    mut facility_profits: Query<&mut FacilityProfit>,
) {
    for transaction in completed_transactions.read() {
        let Some(subject) = transaction.subject else {
            continue;
        };
        let Ok(mut facility_profit) = facility_profits.get_mut(subject) else {
            continue;
        };
        let zoo_cash_change = match (transaction.debit, transaction.credit) {
            (Account::Zoo, _) => -transaction.amount.0,
            (_, Account::Zoo) => transaction.amount.0,
            _ => continue,
        };
        facility_profit.total = Money(facility_profit.total.0.saturating_add(zoo_cash_change));
        facility_profit.transactions = facility_profit.transactions.saturating_add(1);
    }
}
