use bevy::prelude::Entity;

use crate::plugins::economy::{account_transaction_types::Account, money_types::Money};

use super::donation_payment_types::{DonationTotal, PendingDonationPayment};

pub(super) fn accumulate_completed_donation_total(
    current_total: DonationTotal,
    completed_amount: Money,
) -> Option<DonationTotal> {
    if completed_amount.0 <= 0 {
        return None;
    }

    Some(DonationTotal {
        amount: Money(current_total.amount.0.checked_add(completed_amount.0)?),
        count: current_total.count.checked_add(1)?,
    })
}

pub(super) fn completed_transaction_matches_pending_donation_payment(
    pending_payment: PendingDonationPayment,
    debit_account: Account,
    credit_account: Account,
    amount: Money,
    transaction_subject: Option<Entity>,
) -> bool {
    debit_account == Account::Entity(pending_payment.guest)
        && credit_account == Account::Zoo
        && amount == pending_payment.amount
        && transaction_subject == Some(pending_payment.acceptor)
}
