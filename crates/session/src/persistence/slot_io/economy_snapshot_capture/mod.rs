use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure;
use bevy::prelude::Entity;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::economy::monthly_finance_types::FinanceMetric;
use openzt2_zoo_state::economy::monthly_finance_types::{FinancePeriod, FinanceValue};

use super::{
    economy_snapshot_types::{
        EconomySnapshotRecords, EntityEconomySnapshotRecord, ServiceFacilitySnapshot,
    },
    world_snapshot_capture_system_parameters::EconomyCaptureQueries,
};

pub(super) fn capture_economy_snapshot_records_from_live_world(
    world_snapshot_capture_queries: &EconomyCaptureQueries,
    root: Entity,
) -> Result<EconomySnapshotRecords, WorldSnapshotPersistenceFailure> {
    use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
    use openzt2_zoo_state::economy::{
        behavior_transaction_state::EconomyUsage, facility_economy_types::*,
    };
    let (cash, price, admissions, history, policy) = world_snapshot_capture_queries
        .roots
        .get(root)
        .map_err(|_| WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference)?;
    let mut entity_economy_records = world_snapshot_capture_queries
        .entities_with_economy_state
        .iter()
        .filter_map(|entity| {
            if entity
                .get::<WorldMember>()
                .is_none_or(|member| member.root != root)
            {
                return None;
            }
            let profit = entity.get::<FacilityProfit>();
            let opening = entity.get::<OperatingSinceDay>();
            let price_index = entity.get::<FacilityPriceIndex>();
            let maintenance = entity.get::<MaintenanceScheduleIndex>();
            let wallet = entity.get::<Wallet>();
            let price = entity.get::<Price>();
            let service = entity.get::<ServiceFacility>();
            let inventory = entity.get::<Inventory>();
            let usage = entity.get::<EconomyUsage>();
            if profit.is_none()
                && opening.is_none()
                && price_index.is_none()
                && maintenance.is_none()
                && wallet.is_none()
                && price.is_none()
                && service.is_none()
                && inventory.is_none()
                && usage.is_none()
                && entity
                    .get::<openzt2_world_state::world_spawn::world_membership_types::DefinitionId>()
                    .is_none()
            {
                return None;
            }
            Some(EntityEconomySnapshotRecord {
                persistent_identifier: *entity
                    .get::<PersistentId>()
                    .expect("query requires persistent identity"),
                complete_component_state: true,
                opening_zoo_day: opening.map(|opening| opening.0),
                profit: profit.map(|profit| (profit.total, profit.transactions)),
                price_index: price_index.map(|price| price.0),
                maintenance_schedule_index: maintenance.map(|schedule| schedule.0),
                wallet: wallet.map(|wallet| wallet.0),
                price: price.map(|price| price.0),
                service: service.map(|service| ServiceFacilitySnapshot {
                    definition: service.definition,
                    capacity: service.capacity,
                    occupied: 0,
                    payment_trigger: service.payment_trigger,
                }),
                inventory: inventory.map(|stock| {
                    [
                        stock
                            .available
                            .saturating_add(stock.reserved)
                            .min(stock.capacity),
                        0,
                        stock.capacity,
                    ]
                }),
                usage: usage.map(EconomyUsage::counts),
            })
        })
        .collect::<Vec<_>>();
    entity_economy_records.sort_unstable_by_key(|record| record.persistent_identifier.0);

    let lifetime_money =
        |metric| match history.finance_metric_value(metric, FinancePeriod::Lifetime) {
            FinanceValue::Money(amount) => amount,
            FinanceValue::Users(_) => Money::ZERO,
        };
    Ok(EconomySnapshotRecords {
        zoo_cash: cash.0,
        unlimited_cash: Some(policy.is_some_and(|policy| policy.0)),
        admission_price: price.adult,
        admissions_open: admissions.0,
        current_calendar_month: history.current_calendar_month_ordinal(),
        lifetime_income: lifetime_money(FinanceMetric::Income),
        lifetime_expenses: lifetime_money(FinanceMetric::Expenses),
        lifetime_donation_income: lifetime_money(FinanceMetric::DonationIncome),
        lifetime_users: match history
            .finance_metric_value(FinanceMetric::TotalUsers, FinancePeriod::Lifetime)
        {
            FinanceValue::Users(count) => count,
            FinanceValue::Money(_) => 0,
        },
        monthly_finance_history_initialized: history.has_initialized_month(),
        monthly_finance_records: history.retained_month_finance_records().copied().collect(),
        entity_economy_records,
    })
}
