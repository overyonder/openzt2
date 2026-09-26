use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::plugins::{
    economy::account_transaction_types::{
        TransactionCompleted, TransactionKind, TransactionRejected, TransactionRejection,
    },
    guests::guest_simulation_types::Guest,
};

use super::{
    donation_guest_decision_types::DonationProgress,
    donation_opportunity_types::DonationOpportunity,
    donation_payment_calculations::{
        accumulate_completed_donation_total, completed_transaction_matches_pending_donation_payment,
    },
    donation_payment_types::{
        DonationCategoryTotals, DonationCompleted, DonationPaymentRequested, DonationPaymentState,
        DonationRejected, DonationRejection, DonationTotal, PendingDonationPayment,
    },
};

pub(super) fn settle_completed_donation_transactions_and_update_acceptor_totals(
    mut commands: Commands,
    mut completed_transactions: MessageReader<TransactionCompleted>,
    mut payment_operations: Query<(&PendingDonationPayment, &mut DonationPaymentState)>,
    guests: Query<(), (With<Guest>, With<DonationPaymentRequested>)>,
    donation_opportunities: Query<&DonationOpportunity>,
    mut donation_totals: Query<&mut DonationTotal>,
    mut donation_category_totals: Query<&mut DonationCategoryTotals>,
    mut completed_donations: MessageWriter<DonationCompleted>,
    mut rejected_donations: MessageWriter<DonationRejected>,
) {
    // Missing components are inserted after all payments in this run are counted.
    let mut new_donation_totals: BTreeMap<Entity, DonationTotal> = BTreeMap::new();
    let mut new_category_totals: BTreeMap<Entity, DonationCategoryTotals> = BTreeMap::new();
    for transaction in completed_transactions.read() {
        if transaction.kind != TransactionKind::Donation {
            continue;
        }
        let Ok((pending_payment, mut payment_state)) =
            payment_operations.get_mut(transaction.operation)
        else {
            continue;
        };
        if *payment_state != DonationPaymentState::Pending {
            continue;
        }
        *payment_state = DonationPaymentState::Resolved;

        let transaction_matches_payment = completed_transaction_matches_pending_donation_payment(
            *pending_payment,
            transaction.debit,
            transaction.credit,
            transaction.amount,
            transaction.subject,
        );
        if !transaction_matches_payment || guests.get(pending_payment.guest).is_err() {
            reject_pending_donation_payment_and_remove_operation(
                &mut commands,
                &mut rejected_donations,
                *pending_payment,
                transaction.operation,
                DonationRejection::AccountFailure,
            );
            continue;
        }

        let Ok(donation_opportunity) = donation_opportunities.get(pending_payment.subject) else {
            reject_pending_donation_payment_and_remove_operation(
                &mut commands,
                &mut rejected_donations,
                *pending_payment,
                transaction.operation,
                DonationRejection::InvalidSubject,
            );
            continue;
        };
        let updated_total = match donation_totals.get_mut(pending_payment.acceptor) {
            Ok(current_total) => {
                accumulate_completed_donation_total(*current_total, pending_payment.amount)
            }
            Err(_) => match new_donation_totals.get(&pending_payment.acceptor) {
                Some(total) => accumulate_completed_donation_total(*total, pending_payment.amount),
                None => Some(DonationTotal {
                    amount: pending_payment.amount,
                    count: 1,
                }),
            },
        };
        let Some(updated_total) = updated_total else {
            reject_pending_donation_payment_and_remove_operation(
                &mut commands,
                &mut rejected_donations,
                *pending_payment,
                transaction.operation,
                DonationRejection::AccountFailure,
            );
            continue;
        };

        let category_total_was_updated =
            if let Ok(mut totals) = donation_category_totals.get_mut(pending_payment.acceptor) {
                totals
                    .accumulate_completed_donation(
                        donation_opportunity.category,
                        pending_payment.amount,
                    )
                    .is_some()
            } else {
                new_category_totals
                    .entry(pending_payment.acceptor)
                    .or_default()
                    .accumulate_completed_donation(
                        donation_opportunity.category,
                        pending_payment.amount,
                    )
                    .is_some()
            };
        if !category_total_was_updated {
            reject_pending_donation_payment_and_remove_operation(
                &mut commands,
                &mut rejected_donations,
                *pending_payment,
                transaction.operation,
                DonationRejection::AccountFailure,
            );
            continue;
        }

        if let Ok(mut total) = donation_totals.get_mut(pending_payment.acceptor) {
            *total = updated_total;
        } else {
            new_donation_totals.insert(pending_payment.acceptor, updated_total);
        }
        clear_guest_donation_payment_state(&mut commands, pending_payment.guest);
        commands.entity(transaction.operation).despawn();
        completed_donations.write(DonationCompleted {
            guest: pending_payment.guest,
            acceptor: pending_payment.acceptor,
            subject: pending_payment.subject,
            amount: pending_payment.amount,
        });
    }
    for (acceptor, total) in new_donation_totals {
        commands.entity(acceptor).insert(total);
    }
    for (acceptor, totals) in new_category_totals {
        commands.entity(acceptor).insert(totals);
    }
}

pub(super) fn settle_rejected_donation_transactions_and_clear_guest_payment_state(
    mut commands: Commands,
    mut rejected_transactions: MessageReader<TransactionRejected>,
    mut payment_operations: Query<(&PendingDonationPayment, &mut DonationPaymentState)>,
    mut rejected_donations: MessageWriter<DonationRejected>,
) {
    for transaction in rejected_transactions.read() {
        if transaction.kind != TransactionKind::Donation {
            continue;
        }
        let Ok((pending_payment, mut payment_state)) =
            payment_operations.get_mut(transaction.operation)
        else {
            continue;
        };
        if *payment_state != DonationPaymentState::Pending {
            continue;
        }
        *payment_state = DonationPaymentState::Resolved;

        let transaction_matches_payment = completed_transaction_matches_pending_donation_payment(
            *pending_payment,
            transaction.debit,
            transaction.credit,
            transaction.amount,
            transaction.subject,
        );
        let rejection = if transaction_matches_payment {
            map_economy_transaction_rejection_to_donation_rejection(transaction.reason)
        } else {
            DonationRejection::AccountFailure
        };
        reject_pending_donation_payment_and_remove_operation(
            &mut commands,
            &mut rejected_donations,
            *pending_payment,
            transaction.operation,
            rejection,
        );
    }
}

fn reject_pending_donation_payment_and_remove_operation(
    commands: &mut Commands,
    rejected_donations: &mut MessageWriter<DonationRejected>,
    pending_payment: PendingDonationPayment,
    payment_operation: Entity,
    reason: DonationRejection,
) {
    rejected_donations.write(DonationRejected {
        guest: pending_payment.guest,
        acceptor: pending_payment.acceptor,
        reason,
    });
    clear_guest_donation_payment_state(commands, pending_payment.guest);
    commands.entity(payment_operation).despawn();
}

fn clear_guest_donation_payment_state(commands: &mut Commands, guest: Entity) {
    if let Ok(mut guest_entity) = commands.get_entity(guest) {
        guest_entity
            .remove::<DonationProgress>()
            .remove::<DonationPaymentRequested>();
    }
}

const fn map_economy_transaction_rejection_to_donation_rejection(
    transaction_rejection: TransactionRejection,
) -> DonationRejection {
    match transaction_rejection {
        TransactionRejection::InsufficientFunds => DonationRejection::InsufficientFunds,
        // Cancellation did not commit a donation; use the ordinary account
        // rejection cleanup without misreporting insufficient guest funds.
        TransactionRejection::Cancelled
        | TransactionRejection::NonPositive
        | TransactionRejection::MissingAccount
        | TransactionRejection::Overflow => DonationRejection::AccountFailure,
    }
}
