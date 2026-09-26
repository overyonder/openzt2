use crate::AssetId;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiFinanceTableCategoryDefinition {
    pub value_source: UiFinanceTableValueSource,
    pub label: AssetId,
    pub format: AssetId,
}

/// Canonical economy fact projected by one authored finance-table row.
///
/// Source category spellings belong to source lowering. Runtime finance UI
/// borrows one of these closed facts directly from the canonical monthly
/// finance record.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiFinanceTableValueSource {
    AdmissionsCount,
    AdmissionIncome,
    CashGrants,
    DonationIncome,
    FoodDrinkSales,
    RecyclingIncome,
    GiftSales,
    AnimalAdoption,
    AnimalUpkeep,
    Construction,
    Research,
    StaffSalaries,
    Upkeep,
    ClosingCash,
}
