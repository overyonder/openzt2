use bevy::prelude::Resource;

use super::{account_transaction_types::TransactionKind, money_types::Money};

/// Zoo-account totals for one calendar month.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MonthlyFinance {
    pub(crate) month_ordinal: u32,
    pub(crate) opening_cash: Money,
    pub(crate) closing_cash: Money,
    pub(crate) income: Money,
    pub(crate) expenses: Money,
    pub(crate) admission_income: Money,
    pub(crate) cash_grants: Money,
    pub(crate) donation_income: Money,
    pub(crate) food_drink_sales: Money,
    pub(crate) recycling_income: Money,
    pub(crate) gift_sales: Money,
    pub(crate) animal_adoption: Money,
    pub(crate) animal_upkeep: Money,
    pub(crate) construction: Money,
    pub(crate) research: Money,
    pub(crate) staff_salaries: Money,
    pub(crate) upkeep: Money,
    pub(crate) active_users: i32,
    pub(crate) total_users: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FinanceMetric {
    Cash,
    Income,
    Expenses,
    Profit,
    DonationIncome,
    TotalUsers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FinancePeriod {
    Lifetime,
}

const MONTHLY_FINANCE_HISTORY_CAPACITY: usize = 120;
const MONTHLY_FINANCE_HISTORY_CAPACITY_MONTHS: u32 = 120;

/// The most recent ten years of monthly zoo-account totals.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MonthlyFinanceHistory {
    months: [MonthlyFinance; MONTHLY_FINANCE_HISTORY_CAPACITY],
    start: usize,
    len: usize,
    current_calendar_month: u32,
    lifetime_income: Money,
    lifetime_expenses: Money,
    lifetime_donation_income: Money,
    lifetime_users: i64,
    initialized: bool,
}

impl Default for MonthlyFinanceHistory {
    fn default() -> Self {
        Self {
            months: [MonthlyFinance::default(); MONTHLY_FINANCE_HISTORY_CAPACITY],
            start: 0,
            len: 0,
            current_calendar_month: 0,
            lifetime_income: Money::ZERO,
            lifetime_expenses: Money::ZERO,
            lifetime_donation_income: Money::ZERO,
            lifetime_users: 0,
            initialized: false,
        }
    }
}

impl MonthlyFinanceHistory {
    pub(crate) fn current_month_finance(&self) -> Option<&MonthlyFinance> {
        (self.len != 0).then(|| {
            let index = (self.start + self.len - 1) % MONTHLY_FINANCE_HISTORY_CAPACITY;
            &self.months[index]
        })
    }

    pub(crate) fn retained_month_finance_records(
        &self,
    ) -> impl ExactSizeIterator<Item = &MonthlyFinance> {
        (0..self.len)
            .map(|offset| &self.months[(self.start + offset) % MONTHLY_FINANCE_HISTORY_CAPACITY])
    }

    pub(crate) const fn current_calendar_month_ordinal(&self) -> u32 {
        self.current_calendar_month
    }

    pub(crate) const fn has_initialized_month(&self) -> bool {
        self.initialized
    }

    /// Restores saved totals. Invalid or non-contiguous history leaves the
    /// current history unchanged.
    pub(crate) fn restore_saved_monthly_finance_history(
        &mut self,
        months: &[MonthlyFinance],
        current_calendar_month: u32,
        lifetime_income: Money,
        lifetime_expenses: Money,
        lifetime_donation_income: Money,
        lifetime_users: i64,
        initialized: bool,
    ) -> bool {
        let valid_totals = lifetime_income.0 >= 0
            && lifetime_expenses.0 >= 0
            && lifetime_donation_income.0 >= 0
            && lifetime_donation_income <= lifetime_income
            && lifetime_users >= 0;
        let valid_shape = months.len() <= MONTHLY_FINANCE_HISTORY_CAPACITY
            && initialized == !months.is_empty()
            && months.windows(2).all(|pair| {
                pair[0].month_ordinal.checked_add(1) == Some(pair[1].month_ordinal)
                    && pair[0].closing_cash == pair[1].opening_cash
            })
            && months.iter().all(|month| {
                month.income.0 >= 0
                    && month.expenses.0 >= 0
                    && month.admission_income.0 >= 0
                    && month.cash_grants.0 >= 0
                    && month.donation_income.0 >= 0
                    && month.food_drink_sales.0 >= 0
                    && month.recycling_income.0 >= 0
                    && month.gift_sales.0 >= 0
                    && month.animal_adoption.0 >= 0
                    && month.animal_upkeep.0 >= 0
                    && month.construction.0 >= 0
                    && month.research.0 >= 0
                    && month.staff_salaries.0 >= 0
                    && month.upkeep.0 >= 0
                    && month.donation_income <= month.income
                    && month.active_users >= 0
                    && month.total_users >= month.active_users
            });
        if !valid_totals || !valid_shape {
            return false;
        }

        let mut restored = Self {
            current_calendar_month,
            lifetime_income,
            lifetime_expenses,
            lifetime_donation_income,
            lifetime_users,
            initialized,
            ..Self::default()
        };
        months
            .iter()
            .copied()
            .for_each(|month| restored.push_month_finance_record_into_bounded_history(month));
        *self = restored;
        true
    }

    pub(crate) fn finance_metric_value(&self, metric: FinanceMetric, period: FinancePeriod) -> i64 {
        match (metric, period) {
            (FinanceMetric::Income, FinancePeriod::Lifetime) => self.lifetime_income.0,
            (FinanceMetric::Expenses, FinancePeriod::Lifetime) => self.lifetime_expenses.0,
            (FinanceMetric::Profit, FinancePeriod::Lifetime) => self
                .lifetime_income
                .0
                .saturating_sub(self.lifetime_expenses.0),
            (FinanceMetric::DonationIncome, FinancePeriod::Lifetime) => {
                self.lifetime_donation_income.0
            }
            (FinanceMetric::TotalUsers, FinancePeriod::Lifetime) => self.lifetime_users,
            (FinanceMetric::Cash, FinancePeriod::Lifetime) => self
                .current_month_finance()
                .map_or(0, |row| row.closing_cash.0),
        }
    }

    pub(crate) fn enter_calendar_month(&mut self, calendar_month: u32, cash: Money) {
        if self.initialized && self.current_calendar_month == calendar_month {
            return;
        }
        let (active_users, carried_cash) = self
            .current_month_finance()
            .map_or((0, cash), |month| (month.active_users, month.closing_cash));
        let elapsed = if self.initialized {
            calendar_month.saturating_sub(self.current_calendar_month)
        } else {
            1
        };
        let first_month = if self.initialized {
            self.current_month_finance()
                .map_or(0, |month| month.month_ordinal.saturating_add(1))
        } else {
            0
        };
        self.current_calendar_month = calendar_month;
        self.initialized = true;
        for offset in elapsed.saturating_sub(MONTHLY_FINANCE_HISTORY_CAPACITY_MONTHS)..elapsed {
            self.push_month_finance_record_into_bounded_history(MonthlyFinance {
                month_ordinal: first_month.saturating_add(offset),
                opening_cash: carried_cash,
                closing_cash: carried_cash,
                active_users,
                ..MonthlyFinance::default()
            });
        }
        if let Some(current) = self.current_month_finance_mut() {
            current.closing_cash = cash;
        }
    }

    pub(crate) fn record_completed_transaction(
        &mut self,
        kind: TransactionKind,
        delta: i64,
        cash: Money,
    ) {
        if delta >= 0 {
            let amount = Money(delta);
            self.lifetime_income = Money(self.lifetime_income.0.saturating_add(amount.0));
            if kind == TransactionKind::Donation {
                self.lifetime_donation_income =
                    Money(self.lifetime_donation_income.0.saturating_add(amount.0));
            }
        } else {
            let amount = delta.saturating_abs();
            self.lifetime_expenses = Money(self.lifetime_expenses.0.saturating_add(amount));
        }
        let row = self
            .current_month_finance_mut()
            .expect("the economy system enters the current month before recording");
        row.closing_cash = cash;
        if delta >= 0 {
            row.income = Money(row.income.0.saturating_add(delta));
            let category = match kind {
                TransactionKind::Admission => &mut row.admission_income,
                TransactionKind::Donation => &mut row.donation_income,
                TransactionKind::Reward => &mut row.cash_grants,
                _ => return,
            };
            category.0 = category.0.saturating_add(delta);
        } else {
            let amount = delta.saturating_abs();
            row.expenses = Money(row.expenses.0.saturating_add(amount));
            let category = match kind {
                TransactionKind::AnimalAdoption => &mut row.animal_adoption,
                TransactionKind::Construction => &mut row.construction,
                TransactionKind::Research => &mut row.research,
                TransactionKind::StaffHire | TransactionKind::Wage => &mut row.staff_salaries,
                _ => return,
            };
            category.0 = category.0.saturating_add(amount);
        }
    }

    pub(crate) fn record_guest_arrival(&mut self) {
        let row = self
            .current_month_finance_mut()
            .expect("the economy system enters the current month before recording");
        row.active_users = row.active_users.saturating_add(1);
        row.total_users = row.total_users.saturating_add(1);
        self.lifetime_users = self.lifetime_users.saturating_add(1);
    }

    pub(crate) fn record_food_drink_sale(&mut self, amount: Money) {
        let row = self
            .current_month_finance_mut()
            .expect("the economy system enters the current month before recording");
        row.food_drink_sales.0 = row.food_drink_sales.0.saturating_add(amount.0);
    }

    pub(crate) fn record_gift_sale(&mut self, amount: Money) {
        let row = self
            .current_month_finance_mut()
            .expect("the economy system enters the current month before recording");
        row.gift_sales.0 = row.gift_sales.0.saturating_add(amount.0);
    }

    pub(crate) fn record_guest_departure(&mut self) {
        let row = self
            .current_month_finance_mut()
            .expect("the economy system enters the current month before recording");
        row.active_users = row.active_users.saturating_sub(1);
    }

    fn current_month_finance_mut(&mut self) -> Option<&mut MonthlyFinance> {
        (self.len != 0).then(|| {
            let index = (self.start + self.len - 1) % MONTHLY_FINANCE_HISTORY_CAPACITY;
            &mut self.months[index]
        })
    }

    fn push_month_finance_record_into_bounded_history(&mut self, month: MonthlyFinance) {
        let index = (self.start + self.len) % MONTHLY_FINANCE_HISTORY_CAPACITY;
        self.months[index] = month;
        if self.len == MONTHLY_FINANCE_HISTORY_CAPACITY {
            self.start = (self.start + 1) % MONTHLY_FINANCE_HISTORY_CAPACITY;
        } else {
            self.len += 1;
        }
    }
}
