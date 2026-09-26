use bevy::prelude::*;

use crate::plugins::{
    economy::{
        account_transaction_types::{Account, TransactionCompleted, TransactionKind},
        money_types::Money,
    },
    guests::guest_simulation_types::GuestReaction,
};

use super::{
    tour_scoring_types::TransportTripCompleted,
    transport_fare_settlement::complete_paid_transport_trips_and_publish_guest_reactions,
    transport_fare_types::{PendingTransportFare, QueuedFare},
};

fn application_with_transport_fare_settlement() -> App {
    let mut application = App::new();
    application
        .add_message::<TransactionCompleted>()
        .add_message::<TransportTripCompleted>()
        .add_message::<GuestReaction>()
        .add_systems(
            Update,
            complete_paid_transport_trips_and_publish_guest_reactions,
        );
    application
}

#[test]
fn matching_completed_payment_emits_one_trip_and_consumes_fare_operation() {
    let mut application = application_with_transport_fare_settlement();
    let guest_entity = application.world_mut().spawn_empty().id();
    let circuit_entity = application.world_mut().spawn_empty().id();
    let fare_operation = application
        .world_mut()
        .spawn((
            PendingTransportFare {
                guest: guest_entity,
                circuit: circuit_entity,
                score: 420.0,
                rating: 0.42,
            },
            QueuedFare::pending(Money(250)),
        ))
        .id();
    application.world_mut().write_message(TransactionCompleted {
        operation: fare_operation,
        debit: Account::Entity(guest_entity),
        credit: Account::Zoo,
        amount: Money(250),
        kind: TransactionKind::Service,
        subject: Some(circuit_entity),
    });
    application.update();

    assert!(application.world().get_entity(fare_operation).is_err());
    assert_eq!(
        application
            .world()
            .resource::<Messages<TransportTripCompleted>>()
            .len(),
        1,
    );
}

#[test]
fn unrelated_completed_payment_does_not_complete_transport_trip() {
    let mut application = application_with_transport_fare_settlement();
    let guest_entity = application.world_mut().spawn_empty().id();
    let circuit_entity = application.world_mut().spawn_empty().id();
    let fare_operation = application
        .world_mut()
        .spawn((
            PendingTransportFare {
                guest: guest_entity,
                circuit: circuit_entity,
                score: 100.0,
                rating: 0.1,
            },
            QueuedFare::pending(Money(250)),
        ))
        .id();
    application.world_mut().write_message(TransactionCompleted {
        operation: fare_operation,
        debit: Account::Entity(guest_entity),
        credit: Account::Zoo,
        amount: Money(1),
        kind: TransactionKind::Service,
        subject: Some(circuit_entity),
    });
    application.update();

    assert!(application.world().get_entity(fare_operation).is_ok());
    assert!(application
        .world()
        .resource::<Messages<TransportTripCompleted>>()
        .is_empty());
}
