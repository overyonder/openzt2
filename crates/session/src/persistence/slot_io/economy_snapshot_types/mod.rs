use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinance;

#[derive(Debug, Clone, Copy)]
pub(super) struct EntityEconomySnapshotRecord {
    pub persistent_identifier: PersistentId,
    pub complete_component_state: bool,
    pub opening_zoo_day: Option<u32>,
    pub profit: Option<(i64, u32)>,
    pub price_index: Option<u8>,
    pub maintenance_schedule_index: Option<u8>,
    pub wallet: Option<i64>,
    pub price: Option<i64>,
    pub service: Option<ServiceFacilitySnapshot>,
    pub inventory: Option<[u16; 3]>,
    pub usage: Option<[i32; 2]>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ServiceFacilitySnapshot {
    pub definition: openzt2_game_data::AssetId,
    pub capacity: u16,
    pub occupied: u16,
    pub payment_trigger:
        openzt2_game_data::world_definitions::facilities_and_maintenance::FacilityPaymentTrigger,
}

#[derive(Debug)]
pub(super) struct EconomySnapshotRecords {
    pub zoo_cash: Money,
    pub unlimited_cash: Option<bool>,
    pub admission_price: Money,
    pub admissions_open: bool,
    pub current_calendar_month: u32,
    pub lifetime_income: Money,
    pub lifetime_expenses: Money,
    pub lifetime_donation_income: Money,
    pub lifetime_users: i64,
    pub monthly_finance_history_initialized: bool,
    pub monthly_finance_records: Vec<MonthlyFinance>,
    pub entity_economy_records: Vec<EntityEconomySnapshotRecord>,
}
