use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::economy::monthly_finance_types::FinanceMetric;
use openzt2_zoo_state::economy::monthly_finance_types::FinancePeriod;

use super::{
    economy_snapshot_types::{
        EconomySnapshotRecords, EntityEconomySnapshotRecord, ServiceFacilitySnapshot,
    },
    world_snapshot_capture_system_parameters::WorldSnapshotCaptureQueries,
};

pub(super) fn capture_economy_snapshot_records_from_live_world(
    world_snapshot_capture_queries: &WorldSnapshotCaptureQueries,
) -> EconomySnapshotRecords {
    use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
    use openzt2_zoo_state::economy::{
        behavior_transaction_state::EconomyUsage, facility_economy_types::*,
    };
    let mut entity_economy_records = world_snapshot_capture_queries
        .entities_with_economy_state
        .iter()
        .filter_map(|entity| {
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
                profit: profit.map(|profit| (profit.total.0, profit.transactions)),
                price_index: price_index.map(|price| price.0),
                maintenance_schedule_index: maintenance.map(|schedule| schedule.0),
                wallet: wallet.map(|wallet| wallet.0.0),
                price: price.map(|price| price.0.0),
                service: service.map(|service| ServiceFacilitySnapshot {
                    definition: service.definition,
                    capacity: service.capacity,
                    occupied: service.occupied,
                    payment_trigger: service.payment_trigger,
                }),
                inventory: inventory.map(|stock| [stock.available, stock.reserved, stock.capacity]),
                usage: usage.map(EconomyUsage::counts),
            })
        })
        .collect::<Vec<_>>();
    entity_economy_records.sort_unstable_by_key(|record| record.persistent_identifier.0);

    EconomySnapshotRecords {
        zoo_cash: world_snapshot_capture_queries.zoo_cash.0,
        unlimited_cash: Some(world_snapshot_capture_queries.unlimited_cash.is_some()),
        admission_price: world_snapshot_capture_queries.zoo_admission_price.0,
        admissions_open: world_snapshot_capture_queries.zoo_admissions_open_state.0,
        current_calendar_month: world_snapshot_capture_queries
            .monthly_finance_history
            .current_calendar_month_ordinal(),
        lifetime_income: Money(
            world_snapshot_capture_queries
                .monthly_finance_history
                .finance_metric_value(FinanceMetric::Income, FinancePeriod::Lifetime),
        ),
        lifetime_expenses: Money(
            world_snapshot_capture_queries
                .monthly_finance_history
                .finance_metric_value(FinanceMetric::Expenses, FinancePeriod::Lifetime),
        ),
        lifetime_donation_income: Money(
            world_snapshot_capture_queries
                .monthly_finance_history
                .finance_metric_value(FinanceMetric::DonationIncome, FinancePeriod::Lifetime),
        ),
        lifetime_users: world_snapshot_capture_queries
            .monthly_finance_history
            .finance_metric_value(FinanceMetric::TotalUsers, FinancePeriod::Lifetime),
        monthly_finance_history_initialized: world_snapshot_capture_queries
            .monthly_finance_history
            .has_initialized_month(),
        monthly_finance_records: world_snapshot_capture_queries
            .monthly_finance_history
            .retained_month_finance_records()
            .copied()
            .collect(),
        entity_economy_records,
    }
}
