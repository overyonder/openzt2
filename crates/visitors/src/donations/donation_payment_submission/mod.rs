use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_transport::transport_tours::tour_donation::TourDonationOffer;
use openzt2_zoo_state::{
    animal_behavior::behavior_token_types::{BehaviorSelfToken, ClaimedBehaviorSelfToken},
    behavior_task_execution_types::{
        running_actions::{
            mark_behavior_task_for_failure_and_stop_navigation, BehaviorInstructionId,
            RunBehaviorEconomy, RunningBehaviorInstruction, WhenBehaviorAction,
        },
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    economy::{
        account_transaction_types::TransactionDomain,
        behavior_transaction_state::BehaviorTransactionPayment,
    },
    transport_tours::transport_circuit_types::TransportCircuit,
};

use openzt2_zoo_state::donations::donation_guest_decision_types::DonationProgress;

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::economy::account_transaction_types::Account;
use openzt2_zoo_state::economy::account_transaction_types::TransactionKind;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRequest;
use openzt2_zoo_state::economy::facility_economy_types::Wallet;
use openzt2_zoo_state::guests::guest_simulation_types::Guest;

use super::{
    donation_opportunity_types::{DirectDonationSettlement, DonationAcceptor, DonationOpportunity},
    donation_payment_types::{
        DonationPaymentRequested, DonationPaymentState, DonationRejected, DonationRejection,
        DonationRequest, PendingDonationPayment,
    },
};

/// Claims authored end-of-experience donations before transfer submission.
///
/// Chaining this system before the transfer-submission system applies the
/// marker before requests are read there. The submission system can then
/// mutate the marker in place, making duplicate messages for one guest
/// harmless within the same fixed tick without a side registry.
pub(super) fn claim_authorized_direct_donation_requests_before_transfer_submission(
    mut commands: Commands,
    mut requests: MessageReader<DonationRequest>,
    guests: Query<(), (With<Guest>, Without<DonationPaymentRequested>)>,
    acceptors: Query<(), (With<DonationAcceptor>, With<DirectDonationSettlement>)>,
    opportunities: Query<&DonationOpportunity>,
) {
    for request in requests.read() {
        if !request.amount.is_positive()
            || request.subject != request.acceptor
            || guests.get(request.guest).is_err()
            || acceptors.get(request.acceptor).is_err()
            || !opportunities
                .get(request.subject)
                .is_ok_and(|opportunity| request.amount == opportunity.amount)
        {
            continue;
        }

        commands
            .entity(request.guest)
            .insert(DonationPaymentRequested { submitted: false });
    }
}

pub(super) fn submit_validated_donation_requests_to_economy_transfers(
    mut commands: Commands,
    mut donation_requests: MessageReader<DonationRequest>,
    mut guests: Query<
        (
            &Wallet,
            Option<&DonationProgress>,
            &WorldMember,
            Option<&mut DonationPaymentRequested>,
        ),
        With<Guest>,
    >,
    acceptors: Query<(), With<DonationAcceptor>>,
    opportunities: Query<&DonationOpportunity>,
    direct_settlements: Query<(), With<DirectDonationSettlement>>,
    mut transaction_requests: MessageWriter<TransactionRequest>,
    mut rejected_donations: MessageWriter<DonationRejected>,
) {
    for request in donation_requests.read() {
        let Ok((wallet, progress, world_member, submission)) = guests.get_mut(request.guest) else {
            reject_donation_request_and_clear_guest_payment_state(
                &mut commands,
                &mut rejected_donations,
                *request,
                DonationRejection::RuleDenied,
            );
            continue;
        };

        let progress_matches_request = progress.is_some_and(|progress| {
            progress.acceptor == request.acceptor
                && progress.subject == request.subject
                && progress.amount == request.amount
        });
        // Authorized experiences settle their authored donation at completion,
        // so they have no donation-box docking progress. Their direct request
        // is valid only when the subject is its own live acceptor and the
        // authored amount still agrees exactly.
        #[allow(
            clippy::suspicious_operation_groupings,
            reason = "direct settlement requires a submission and the absence of docking progress"
        )]
        let direct_settlement_matches_request = progress.is_none()
            && submission.is_some()
            && request.subject == request.acceptor
            && direct_settlements.get(request.subject).is_ok()
            && acceptors.get(request.acceptor).is_ok()
            && opportunities
                .get(request.subject)
                .is_ok_and(|opportunity| request.amount == opportunity.amount);
        let rejection = if !(progress_matches_request || direct_settlement_matches_request)
            || !request.amount.is_positive()
        {
            Some(DonationRejection::RuleDenied)
        } else if acceptors.get(request.acceptor).is_err() {
            Some(DonationRejection::InvalidAcceptor)
        } else if wallet.0 < request.amount {
            Some(DonationRejection::InsufficientFunds)
        } else {
            None
        };
        if let Some(reason) = rejection {
            reject_donation_request_and_clear_guest_payment_state(
                &mut commands,
                &mut rejected_donations,
                *request,
                reason,
            );
            continue;
        }
        if submission
            .as_deref()
            .is_some_and(|submission| submission.submitted)
        {
            continue;
        }
        if let Some(mut submission) = submission {
            submission.submitted = true;
        }

        let payment_operation = commands
            .spawn((
                *world_member,
                PendingDonationPayment {
                    guest: request.guest,
                    acceptor: request.acceptor,
                    subject: request.subject,
                    amount: request.amount,
                },
                DonationPaymentState::Pending,
            ))
            .id();
        transaction_requests.write(TransactionRequest {
            domain: TransactionDomain::Simulation,
            operation: payment_operation,
            debit: Account::Entity(request.guest),
            credit: Account::Zoo,
            amount: request.amount,
            kind: TransactionKind::Donation,
            subject: Some(request.acceptor),
        });
    }
}

fn reject_donation_request_and_clear_guest_payment_state(
    commands: &mut Commands,
    rejected_donations: &mut MessageWriter<DonationRejected>,
    request: DonationRequest,
    reason: DonationRejection,
) {
    rejected_donations.write(DonationRejected {
        guest: request.guest,
        acceptor: request.acceptor,
        reason,
    });
    if let Ok(mut guest) = commands.get_entity(request.guest) {
        guest
            .remove::<DonationProgress>()
            .remove::<DonationPaymentRequested>();
    }
}

#[derive(Component)]
pub(super) struct TourDonationInstruction(pub(super) BehaviorInstructionId);

#[allow(clippy::type_complexity)]
pub(super) fn submit_authored_tour_donation(
    action: On<WhenBehaviorAction<RunBehaviorEconomy>>,
    mut commands: Commands,
    guests: Query<
        (
            &WorldMember,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorEconomy,
        ),
        (
            With<Guest>,
            Without<PendingBehaviorTaskFailure>,
            Without<DonationPaymentRequested>,
        ),
    >,
    offers: Query<(
        &TourDonationOffer,
        &BehaviorSelfToken,
        &ClaimedBehaviorSelfToken,
        &ChildOf,
    )>,
    acceptors: Query<&WorldMember, With<DonationAcceptor>>,
    circuits: Query<&WorldMember, (With<TransportCircuit>, With<DonationOpportunity>)>,
    mut payments: MessageWriter<TransactionRequest>,
) {
    let guest = action.entity;
    let Ok((world, task, instruction, economy)) = guests.get(guest) else {
        return;
    };
    if action.instruction != instruction.id()
        || !instruction.is_current(task)
        || economy.transaction != AssetId::from_key("donate")
    {
        return;
    }
    let Some((offer, token, claim, owner)) = task.object.and_then(|object| offers.get(object).ok())
    else {
        return;
    };
    if owner.parent() != guest || claim.execution_id != task.execution_id {
        return;
    }
    let acceptor = task.target.filter(|&target| {
        acceptors
            .get(target)
            .is_ok_and(|acceptor| acceptor.root == world.root)
    });
    let circuit_available = circuits
        .get(offer.circuit)
        .is_ok_and(|circuit| circuit.root == world.root);
    let Some(acceptor) = acceptor.filter(|_| {
        circuit_available
            && offer.cost_override.is_positive()
            && token.name == AssetId::from_key("t_donationtour")
    }) else {
        mark_behavior_task_for_failure_and_stop_navigation(guest, &mut commands);
        return;
    };
    commands
        .entity(guest)
        .insert(DonationPaymentRequested { submitted: true });
    let operation = commands
        .spawn((
            *world,
            PendingDonationPayment {
                guest,
                acceptor,
                subject: offer.circuit,
                amount: offer.cost_override,
            },
            DonationPaymentState::Pending,
            TourDonationInstruction(instruction.id()),
            BehaviorTransactionPayment {
                actor: guest,
                facility: acceptor,
                transaction: offer.transaction_tracker,
            },
        ))
        .id();
    payments.write(TransactionRequest {
        domain: TransactionDomain::Simulation,
        operation,
        debit: Account::Entity(guest),
        credit: Account::Zoo,
        amount: offer.cost_override,
        kind: TransactionKind::Donation,
        subject: Some(acceptor),
    });
}
