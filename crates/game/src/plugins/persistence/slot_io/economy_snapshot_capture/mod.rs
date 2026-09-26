use crate::plugins::economy::{
    money_types::Money,
    monthly_finance_types::{FinanceMetric, FinancePeriod},
};

use super::{
    economy_snapshot_types::{EconomySnapshotRecords, FacilityEconomySnapshotRecord},
    world_snapshot_capture_system_parameters::WorldSnapshotCaptureQueries,
};

pub(super) fn capture_economy_snapshot_records_from_live_world(
    world_snapshot_capture_queries: &WorldSnapshotCaptureQueries,
) -> EconomySnapshotRecords {
    let mut facility_economy_snapshot_records = world_snapshot_capture_queries
        .facilities_with_economy_state
        .iter()
        .map(
            |(persistent_identifier, operating_since_day, facility_profit)| {
                FacilityEconomySnapshotRecord {
                    persistent_identifier: *persistent_identifier,
                    opening_zoo_day: operating_since_day.0,
                    total_profit_cents: facility_profit.total.0,
                    transaction_count: facility_profit.transactions,
                }
            },
        )
        .collect::<Vec<_>>();
    facility_economy_snapshot_records.sort_unstable_by_key(|record| record.persistent_identifier.0);

    EconomySnapshotRecords {
        zoo_cash: world_snapshot_capture_queries.zoo_cash.0,
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
        facility_economy_records: facility_economy_snapshot_records,
    }
}
