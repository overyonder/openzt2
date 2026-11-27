use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestMemoryKind;

use openzt2_zoo_state::economy::account_transaction_types::Account;
use openzt2_zoo_state::economy::account_transaction_types::TransactionKind;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRejected;
use openzt2_zoo_state::economy::account_transaction_types::{
    TransactionCompleted, TransactionDomain,
};
use openzt2_zoo_state::guests::guest_simulation_types::GuestReaction;

use crate::transport_tours::transport_fare_types::{FareGuest, PendingTransportFare, QueuedFare};
use openzt2_zoo_state::transport_tours::tour_scoring_types::TransportTripCompleted;

pub(super) fn complete_paid_transport_trips_and_publish_guest_reactions(
    mut completed_transactions: MessageReader<TransactionCompleted>,
    mut pending_transport_fares: Query<(&FareGuest, &PendingTransportFare, &mut QueuedFare)>,
    mut completed_transport_trips: MessageWriter<TransportTripCompleted>,
    mut guest_reactions: MessageWriter<GuestReaction>,
    mut commands: Commands,
) {
    for completed_transaction in completed_transactions.read() {
        let Ok((guest, pending_fare, mut queued_fare)) =
            pending_transport_fares.get_mut(completed_transaction.operation)
        else {
            continue;
        };
        if queued_fare.settled
            || completed_transaction.domain != TransactionDomain::Simulation
            || completed_transaction.debit != Account::Entity(guest.0)
            || completed_transaction.credit != Account::Zoo
            || completed_transaction.amount != queued_fare.amount
            || completed_transaction.kind != TransactionKind::Service
            || completed_transaction.subject != Some(pending_fare.circuit)
        {
            continue;
        }
        queued_fare.settled = true;
        publish_transport_trip(
            guest.0,
            pending_fare,
            &mut completed_transport_trips,
            &mut guest_reactions,
        );
        commands.entity(completed_transaction.operation).despawn();
    }
}

pub(super) fn discard_rejected_transport_fares(
    mut rejected_transactions: MessageReader<TransactionRejected>,
    mut pending_transport_fares: Query<(&FareGuest, &PendingTransportFare, &mut QueuedFare)>,
    mut commands: Commands,
) {
    for rejected_transaction in rejected_transactions.read() {
        let Ok((guest, pending_fare, mut queued_fare)) =
            pending_transport_fares.get_mut(rejected_transaction.operation)
        else {
            continue;
        };
        if queued_fare.settled
            || rejected_transaction.domain != TransactionDomain::Simulation
            || rejected_transaction.debit != Account::Entity(guest.0)
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

pub(super) fn publish_transport_trip(
    guest: Entity,
    pending_fare: &PendingTransportFare,
    completed_transport_trips: &mut MessageWriter<TransportTripCompleted>,
    guest_reactions: &mut MessageWriter<GuestReaction>,
) {
    completed_transport_trips.write(TransportTripCompleted {
        guest,
        station: pending_fare.station,
        circuit: pending_fare.circuit,
        score: pending_fare.score,
        rating: pending_fare.rating,
    });
    guest_reactions.write(GuestReaction {
        guest,
        subject: pending_fare.circuit,
        kind: GuestMemoryKind::Tour,
        satisfaction_delta_permille: 0,
        education_delta_permille: 0,
    });
}
