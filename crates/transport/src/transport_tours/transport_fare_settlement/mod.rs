use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestMemoryKind;

use openzt2_zoo_state::economy::account_transaction_types::Account;
use openzt2_zoo_state::economy::account_transaction_types::TransactionCompleted;
use openzt2_zoo_state::economy::account_transaction_types::TransactionKind;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRejected;
use openzt2_zoo_state::guests::guest_simulation_types::GuestReaction;

use crate::transport_tours::transport_fare_types::PendingTransportFare;
use crate::transport_tours::transport_fare_types::QueuedFare;
use openzt2_zoo_state::transport_tours::tour_scoring_types::TransportTripCompleted;

pub(super) fn complete_paid_transport_trips_and_publish_guest_reactions(
    mut completed_transactions: MessageReader<TransactionCompleted>,
    mut pending_transport_fares: Query<(&PendingTransportFare, &mut QueuedFare)>,
    mut completed_transport_trips: MessageWriter<TransportTripCompleted>,
    mut guest_reactions: MessageWriter<GuestReaction>,
    mut commands: Commands,
) {
    for completed_transaction in completed_transactions.read() {
        let Ok((pending_fare, mut queued_fare)) =
            pending_transport_fares.get_mut(completed_transaction.operation)
        else {
            continue;
        };
        if queued_fare.settled
            || completed_transaction.debit != Account::Entity(pending_fare.guest)
            || completed_transaction.credit != Account::Zoo
            || completed_transaction.amount != queued_fare.amount
            || completed_transaction.kind != TransactionKind::Service
            || completed_transaction.subject != Some(pending_fare.circuit)
        {
            continue;
        }
        queued_fare.settled = true;
        completed_transport_trips.write(TransportTripCompleted {
            guest: pending_fare.guest,
            circuit: pending_fare.circuit,
            score: pending_fare.score,
            rating: pending_fare.rating,
        });
        guest_reactions.write(GuestReaction {
            view_event: None,
            guest: pending_fare.guest,
            subject: pending_fare.circuit,
            kind: GuestMemoryKind::Tour,
            satisfaction_delta_permille: pending_fare.rating.clamp(0.0, i16::MAX as f32) as i16,
            education_delta_permille: 0,
        });
        commands.entity(completed_transaction.operation).despawn();
    }
}

pub(super) fn discard_rejected_transport_fares(
    mut rejected_transactions: MessageReader<TransactionRejected>,
    mut pending_transport_fares: Query<(&PendingTransportFare, &mut QueuedFare)>,
    mut commands: Commands,
) {
    for rejected_transaction in rejected_transactions.read() {
        let Ok((pending_fare, mut queued_fare)) =
            pending_transport_fares.get_mut(rejected_transaction.operation)
        else {
            continue;
        };
        if queued_fare.settled
            || rejected_transaction.debit != Account::Entity(pending_fare.guest)
            || rejected_transaction.credit != Account::Zoo
            || rejected_transaction.amount != queued_fare.amount
            || rejected_transaction.kind != TransactionKind::Service
            || rejected_transaction.subject != Some(pending_fare.circuit)
        {
            continue;
        }
        queued_fare.settled = true;
        commands.entity(rejected_transaction.operation).despawn();
    }
}
