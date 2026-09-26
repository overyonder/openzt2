use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::plugins::economy::money_types::Money;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DonationTotal {
    pub(crate) amount: Money,
    pub(crate) count: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct DonationCategoryTotal {
    category: AssetId,
    total: DonationTotal,
}

/// Donation totals by category for one acceptor.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DonationCategoryTotals {
    totals: Vec<DonationCategoryTotal>,
}

impl DonationCategoryTotals {
    pub(super) fn accumulate_completed_donation(
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

    pub(crate) fn aggregate_for_category(
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
pub(super) struct PendingDonationPayment {
    pub(super) guest: Entity,
    pub(super) acceptor: Entity,
    pub(super) subject: Entity,
    pub(super) amount: Money,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DonationPaymentRequested {
    /// Mutated in place by the economy submission system. This makes duplicate
    /// public requests for one guest harmless before deferred commands create
    /// the operation entity.
    pub(super) submitted: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DonationPaymentState {
    Pending,
    Resolved,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DonationRequest {
    pub(crate) guest: Entity,
    pub(crate) acceptor: Entity,
    pub(crate) subject: Entity,
    pub(crate) amount: Money,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DonationCompleted {
    pub(super) guest: Entity,
    pub(super) acceptor: Entity,
    pub(super) subject: Entity,
    pub(super) amount: Money,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DonationRejected {
    pub(super) guest: Entity,
    pub(super) acceptor: Entity,
    pub(super) reason: DonationRejection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DonationRejection {
    InvalidAcceptor,
    InvalidSubject,
    Unreachable,
    InsufficientFunds,
    RuleDenied,
    AccountFailure,
}
