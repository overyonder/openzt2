use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;

use super::{
    account_transaction_settlement::RecordedAccountMovement,
    facility_economy_types::{FacilityProfit, OperatingSinceDay, ServiceFacility},
    money_types::Money,
};

pub(super) fn initialize_new_service_facilities_with_operating_age_and_profit(
    mut commands: Commands,
    clocks: Query<&ZooClock>,
    facilities: Query<
        (Entity, &WorldMember, Option<&FacilityProfit>),
        (With<ServiceFacility>, Without<OperatingSinceDay>),
    >,
) {
    for (facility, member, profit) in &facilities {
        let Ok(clock) = clocks.get(member.root) else {
            continue;
        };
        let mut entity = commands.entity(facility);
        entity.insert(OperatingSinceDay(clock.absolute_day));
        if profit.is_none() {
            entity.insert(FacilityProfit::default());
        }
    }
}

pub(super) fn record_completed_transaction_results_for_attributed_facilities<const PLAYER: bool>(
    mut completed_transactions: MessageReader<RecordedAccountMovement<PLAYER>>,
    mut facility_profits: Query<&mut FacilityProfit>,
) {
    for transaction in completed_transactions.read() {
        let Some(subject) = transaction.subject else {
            continue;
        };
        let Ok(mut facility_profit) = facility_profits.get_mut(subject) else {
            continue;
        };
        facility_profit.total = Money(
            facility_profit
                .total
                .0
                .saturating_add(transaction.zoo_delta.0),
        );
        facility_profit.transactions = facility_profit.transactions.saturating_add(1);
    }
}
