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
    assert!(
        application
            .world()
            .resource::<Messages<TransactionRejected>>()
            .is_empty()
    );
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
    assert!(
        application
            .world()
            .resource::<Messages<TransactionCompleted>>()
            .is_empty()
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

#[test]
fn source_global_operations_preserve_independent_ledger_and_atomic_replay() {
    use openzt2_world_state::simulation_time::simulation_clock_types::ZooCalendar;
    use openzt2_zoo_state::economy::general_economy_types::{
        GeneralEconomyLedger, GeneralEconomyTransaction,
    };
    use openzt2_zoo_state::economy::monthly_finance_types::{FinanceMetric, FinancePeriod};

    let mut application = create_application_with_account_transaction_settlement();
    application.insert_resource(ZooCash(Money(3_624_000)));
    application.insert_resource(ZooCalendar {
        year: 1,
        month: 1,
        day: 1,
    });
    application.insert_resource(GeneralEconomyLedger::from_starting_balance(
        12,
        Money(3_706_800),
    ));
    let build = application
        .world_mut()
        .spawn(GeneralEconomyTransaction::Build)
        .id();
    let build_request = TransactionRequest {
        operation: build,
        debit: Account::Zoo,
        credit: Account::External,
        amount: Money(1_000),
        kind: TransactionKind::Construction,
        subject: None,
    };
    application.world_mut().write_message(build_request);
    application.world_mut().write_message(build_request);
    application.update();

    let guest = application.world_mut().spawn(Wallet(Money(2_000))).id();
    let service = application.world_mut().spawn_empty().id();
    application.world_mut().write_message(TransactionRequest {
        operation: service,
        debit: Account::Entity(guest),
        credit: Account::Zoo,
        amount: Money(500),
        kind: TransactionKind::Service,
        subject: None,
    });
    let rejected = application
        .world_mut()
        .spawn(GeneralEconomyTransaction::Research)
        .id();
    application.world_mut().write_message(TransactionRequest {
        operation: rejected,
        debit: Account::Zoo,
        credit: Account::External,
        amount: Money(4_000_000),
        kind: TransactionKind::Research,
        subject: None,
    });
    application.world_mut().write_message(build_request);
    application.update();
    assert_eq!(
        application.world().resource::<ZooCash>().0,
        Money(3_623_500)
    );
    assert_eq!(
        application
            .world()
            .resource::<GeneralEconomyLedger>()
            .cash(),
        Some(Money(3_705_800))
    );

    application.world_mut().resource_mut::<ZooCalendar>().month = 2;
    let compensation = application
        .world_mut()
        .spawn(GeneralEconomyTransaction::Build)
        .id();
    application.world_mut().write_message(TransactionRequest {
        operation: compensation,
        debit: Account::External,
        credit: Account::Zoo,
        amount: Money(1_000),
        kind: TransactionKind::Construction,
        subject: None,
    });
    application.update();
    let ledger = application.world().resource::<GeneralEconomyLedger>();
    assert_eq!(ledger.cash(), Some(Money(3_706_800)));
    assert_eq!(ledger.history().retained_month_finance_records().len(), 2);
    assert_eq!(
        ledger
            .history()
            .finance_metric_value(FinanceMetric::Income, FinancePeriod::Lifetime),
        1_000
    );
    assert_eq!(
        ledger
            .history()
            .finance_metric_value(FinanceMetric::Expenses, FinancePeriod::Lifetime),
        1_000
    );
    assert_eq!(
        ledger
            .history()
            .finance_metric_value(FinanceMetric::TotalUsers, FinancePeriod::Lifetime),
        0
    );
    assert!(
        ledger
            .history()
            .retained_month_finance_records()
            .all(|month| month.construction == Money::ZERO
                && month.active_users == 0
                && month.total_users == 0)
    );

    application.insert_resource(GeneralEconomyLedger::from_starting_balance(
        13,
        Money(i64::MAX),
    ));
    let overflow = application
        .world_mut()
        .spawn(GeneralEconomyTransaction::Admission)
        .id();
    let zoo_before = application.world().resource::<ZooCash>().0;
    application.world_mut().write_message(TransactionRequest {
        operation: overflow,
        debit: Account::Entity(guest),
        credit: Account::Zoo,
        amount: Money(1),
        kind: TransactionKind::Admission,
        subject: None,
    });
    application.update();
    assert_eq!(
        application.world().get::<Wallet>(guest).unwrap().0,
        Money(1_500)
    );
    assert_eq!(application.world().resource::<ZooCash>().0, zoo_before);
    assert_eq!(
        application
            .world()
            .resource::<GeneralEconomyLedger>()
            .cash(),
        Some(Money(i64::MAX))
    );
}
