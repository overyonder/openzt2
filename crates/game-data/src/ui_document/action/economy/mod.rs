//! Zoo admissions, cash, facility price, maintenance schedule, and sell-quote actions.

use super::UiTrigger;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiEconomyActionRecord {
    pub trigger: UiTrigger,
    pub action: UiEconomyAction,
}
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub enum UiEconomyAction {
    SetZooAdmissionPriceBand { price_band_index: i32 },
    SetZooAdmissionsOpen { open: bool },
    GrantZooCash { amount: i64 },
    SetSelectedFacilityPriceIndex { price_index: u8 },
    SetSelectedMaintenanceSchedule { maintenance_schedule_index: u8 },
    RefreshSelectedEntitySellQuote,
}
