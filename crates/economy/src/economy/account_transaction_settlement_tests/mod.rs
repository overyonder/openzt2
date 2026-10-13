use bevy::prelude::*;

use super::{
    account_transaction_settlement::settle_requested_account_transactions,
    account_transaction_types::{
        Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest,
    },
    facility_economy_types::Wallet,
    money_types::Money,
    zoo_cash_types::ZooCash,
};

fn create_application_with_account_transaction_settlement() -> App {
    let mut application = App::new();
    application
        .add_message::<TransactionRequest>()
        .add_message::<TransactionCompleted>()
        .add_message::<TransactionRejected>()
        .insert_resource(ZooCash(Money(1_000)))
        .add_systems(Update, settle_requested_account_transactions);
    application
}

#[test]
fn entity_to_zoo_transaction_commits_both_account_balances_once() {
    let mut application = create_application_with_account_transaction_settlement();
    let guest = application.world_mut().spawn(Wallet(Money(500))).id();
    let operation = application.world_mut().spawn_empty().id();
    application.world_mut().write_message(TransactionRequest {
        operation,
        debit: Account::Entity(guest),
        credit: Account::Zoo,
        amount: Money(125),
        kind: TransactionKind::Admission,
        subject: Some(guest),
    });

    application.update();

    assert_eq!(
        application.world().get::<Wallet>(guest),
        Some(&Wallet(Money(375)))
    );
    assert_eq!(application.world().resource::<ZooCash>().0, Money(1_125));
    assert_eq!(
        application
            .world()
            .resource::<Messages<TransactionCompleted>>()
            .len(),
        1
    );
    assert!(application
        .world()
        .resource::<Messages<TransactionRejected>>()
        .is_empty());
}

#[test]
fn rejected_transaction_leaves_every_account_balance_unchanged() {
    let mut application = create_application_with_account_transaction_settlement();
    let guest = application.world_mut().spawn(Wallet(Money(20))).id();
    let operation = application.world_mut().spawn_empty().id();
    application.world_mut().write_message(TransactionRequest {
        operation,
        debit: Account::Entity(guest),
        credit: Account::Zoo,
        amount: Money(21),
        kind: TransactionKind::Purchase,
        subject: None,
    });

    application.update();

    assert_eq!(
        application.world().get::<Wallet>(guest),
        Some(&Wallet(Money(20)))
    );
    assert_eq!(application.world().resource::<ZooCash>().0, Money(1_000));
    assert!(application
        .world()
        .resource::<Messages<TransactionCompleted>>()
        .is_empty());
    assert_eq!(
        application
            .world()
            .resource::<Messages<TransactionRejected>>()
            .len(),
        1
    );
}

#[test]
fn transaction_request_order_controls_competing_debits() {
    let mut application = create_application_with_account_transaction_settlement();
    let guest = application.world_mut().spawn(Wallet(Money(100))).id();
    for amount in [Money(80), Money(30)] {
        let operation = application.world_mut().spawn_empty().id();
        application.world_mut().write_message(TransactionRequest {
            operation,
            debit: Account::Entity(guest),
            credit: Account::Zoo,
            amount,
            kind: TransactionKind::Service,
            subject: Some(guest),
        });
    }

    application.update();

    assert_eq!(
        application.world().get::<Wallet>(guest),
        Some(&Wallet(Money(20)))
    );
    assert_eq!(application.world().resource::<ZooCash>().0, Money(1_080));
    assert_eq!(
        application
            .world()
            .resource::<Messages<TransactionCompleted>>()
            .len(),
        1
    );
    assert_eq!(
        application
            .world()
            .resource::<Messages<TransactionRejected>>()
            .len(),
        1
    );
}

#[test]
fn repeated_transaction_operation_is_settled_exactly_once() {
    let mut application = create_application_with_account_transaction_settlement();
    let guest = application.world_mut().spawn(Wallet(Money(100))).id();
    let operation = application.world_mut().spawn_empty().id();
    let request = TransactionRequest {
        operation,
        debit: Account::Entity(guest),
        credit: Account::Zoo,
        amount: Money(25),
        kind: TransactionKind::Service,
        subject: Some(guest),
    };
    application.world_mut().write_message(request);
    application.world_mut().write_message(request);

    application.update();

    assert_eq!(
        application.world().get::<Wallet>(guest),
        Some(&Wallet(Money(75)))
    );
    assert_eq!(application.world().resource::<ZooCash>().0, Money(1_025));
    assert_eq!(
        application
            .world()
            .resource::<Messages<TransactionCompleted>>()
            .len(),
        1
    );
}
