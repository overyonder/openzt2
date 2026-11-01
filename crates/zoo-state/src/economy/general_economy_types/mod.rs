use bevy::prelude::{Component, Resource};

use super::{
    money_types::Money,
    monthly_finance_types::{FinanceMetric, FinancePeriod, MonthlyFinanceHistory},
};

/// Identifies a payment issued by a zoo-wide operation.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneralEconomyTransaction {
    Build,
    Destroy,
    Terraform,
    BiomeChange,
    WaterChange,
    Admission,
    Research,
    CashGrant,
}

/// Zoo-wide transactions have their own balance and monthly totals.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GeneralEconomyLedger {
    history: MonthlyFinanceHistory,
}

impl GeneralEconomyLedger {
    pub fn from_starting_balance(calendar_month: u32, cash: Money) -> Self {
        let mut history = MonthlyFinanceHistory::default();
        history.enter_calendar_month(calendar_month, cash);
        Self { history }
    }

    pub fn from_saved(history: MonthlyFinanceHistory) -> Option<Self> {
        let aggregate_only = history
            .finance_metric_value(FinanceMetric::DonationIncome, FinancePeriod::Lifetime)
            == 0
            && history.finance_metric_value(FinanceMetric::TotalUsers, FinancePeriod::Lifetime)
                == 0
            && history.retained_month_finance_records().all(|month| {
                month.active_users == 0
                    && month.total_users == 0
                    && [
                        month.admission_income,
                        month.cash_grants,
                        month.donation_income,
                        month.food_drink_sales,
                        month.recycling_income,
                        month.gift_sales,
                        month.animal_adoption,
                        month.animal_upkeep,
                        month.construction,
                        month.research,
                        month.staff_salaries,
                        month.upkeep,
                    ]
                    .into_iter()
                    .all(|value| value == Money::ZERO)
            });
        aggregate_only.then_some(Self { history })
    }

    pub const fn history(&self) -> &MonthlyFinanceHistory {
        &self.history
    }

    pub fn cash(&self) -> Option<Money> {
        self.history
            .current_month_finance()
            .map(|month| month.closing_cash)
    }

    pub fn enter_calendar_month(&mut self, calendar_month: u32) {
        if let Some(cash) = self.cash() {
            self.history.enter_calendar_month(calendar_month, cash);
        }
    }

    pub fn cash_after_transaction(&self, delta: i64) -> Option<Money> {
        self.cash()?.checked_add(Money(delta))
    }

    pub fn record_completed_transaction(&mut self, delta: i64) -> Option<()> {
        let cash = self.cash_after_transaction(delta)?;
        self.history.record_account_cash_movement(delta, cash);
        Some(())
    }
}
