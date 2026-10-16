use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::economy::money_types::Money;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DonationTotal {
    pub amount: Money,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct DonationCategoryTotal {
    category: AssetId,
    total: DonationTotal,
}

/// Donation totals by category for one acceptor.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct DonationCategoryTotals {
    totals: Vec<DonationCategoryTotal>,
}

impl DonationCategoryTotals {
    pub fn iter(&self) -> impl Iterator<Item = (AssetId, DonationTotal)> + '_ {
        self.totals
            .iter()
            .map(|entry| (entry.category, entry.total))
    }

    /// Restores completed totals without replaying payments or changing cash.
    pub fn from_completed_totals(
        totals: impl IntoIterator<Item = (AssetId, DonationTotal)>,
    ) -> Option<Self> {
        let mut restored = Self::default();
        for (category, total) in totals {
            if total.amount.0 <= 0
                || total.count == 0
                || restored
                    .totals
                    .iter()
                    .any(|entry| entry.category == category)
            {
                return None;
            }
            restored
                .totals
                .push(DonationCategoryTotal { category, total });
        }
        restored.aggregate_for_category(None)?;
        Some(restored)
    }

    pub fn accumulate_completed_donation(
        &mut self,
        category: AssetId,
        amount: Money,
    ) -> Option<()> {
        if let Some(entry) = self
            .totals
            .iter_mut()
            .find(|entry| entry.category == category)
        {
            let next_amount = entry.total.amount.0.checked_add(amount.0)?;
            let next_count = entry.total.count.checked_add(1)?;
            entry.total.amount.0 = next_amount;
            entry.total.count = next_count;
        } else {
            self.totals.push(DonationCategoryTotal {
                category,
                total: DonationTotal { amount, count: 1 },
            });
        }
        Some(())
    }

    pub fn aggregate_for_category(
        &self,
        requested_category: Option<AssetId>,
    ) -> Option<DonationTotal> {
        self.totals
            .iter()
            .filter(|entry| requested_category.is_none_or(|category| entry.category == category))
            .try_fold(DonationTotal::default(), |aggregate, entry| {
                Some(DonationTotal {
                    amount: Money(aggregate.amount.0.checked_add(entry.total.amount.0)?),
                    count: aggregate.count.checked_add(entry.total.count)?,
                })
            })
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingDonationPayment {
    pub guest: Entity,
    pub acceptor: Entity,
    pub subject: Entity,
    pub amount: Money,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DonationPaymentRequested {
    /// Mutated in place by the economy submission system. This makes duplicate
    /// public requests for one guest harmless before deferred commands create
    /// the operation entity.
    pub submitted: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum DonationPaymentState {
    Pending,
    Resolved,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DonationRequest {
    pub guest: Entity,
    pub acceptor: Entity,
    pub subject: Entity,
    pub amount: Money,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DonationCompleted {
    pub guest: Entity,
    pub acceptor: Entity,
    pub subject: Entity,
    pub amount: Money,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DonationRejected {
    pub guest: Entity,
    pub acceptor: Entity,
    pub reason: DonationRejection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DonationRejection {
    InvalidAcceptor,
    InvalidSubject,
    Unreachable,
    InsufficientFunds,
    RuleDenied,
    AccountFailure,
}
