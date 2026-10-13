use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinance;

#[derive(Debug, Clone, Copy)]
pub(super) struct FacilityEconomySnapshotRecord {
    pub persistent_identifier: PersistentId,
    pub opening_zoo_day: u32,
    pub total_profit_cents: i64,
    pub transaction_count: u32,
}

#[derive(Debug)]
pub(super) struct EconomySnapshotRecords {
    pub zoo_cash: Money,
    pub admission_price: Money,
    pub admissions_open: bool,
    pub current_calendar_month: u32,
    pub lifetime_income: Money,
    pub lifetime_expenses: Money,
    pub lifetime_donation_income: Money,
    pub lifetime_users: i64,
    pub monthly_finance_history_initialized: bool,
    pub monthly_finance_records: Vec<MonthlyFinance>,
    pub facility_economy_records: Vec<FacilityEconomySnapshotRecord>,
}
