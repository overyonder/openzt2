use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

fn zoo_root(application: &App) -> Entity {
    application
        .world()
        .iter_entities()
        .find(|entity| entity.contains::<WorldRoot>())
        .unwrap()
        .id()
}

use super::{
    account_transaction_settlement::{
        settle_requested_account_transactions, stage_requested_account_transactions,
    },
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
        .init_resource::<super::account_transaction_settlement::AccountRequestCursor>()
        .add_message::<openzt2_zoo_state::economy::account_transaction_types::RecordedAccountMovement<true>>()
        .add_message::<openzt2_zoo_state::economy::account_transaction_types::RecordedAccountMovement<false>>()
        .add_message::<TransactionCompleted>()
        .add_message::<TransactionRejected>()
        .add_systems(
            Update,
            (
                super::monthly_finance_recording::enter_published_zoo_finance_month,
                stage_requested_account_transactions::<false>,
                settle_requested_account_transactions::<false>,
                stage_requested_account_transactions::<true>,
                settle_requested_account_transactions::<true>,
            )
                .chain(),
        );
    application.world_mut().spawn((
        WorldRoot {
            scenario: Default::default(),
        },
        ZooCash(Money::from_cents(1_000)),
    ));
    application
}

#[test]
fn entity_to_zoo_transaction_commits_both_account_balances_once() {
    let mut application = create_application_with_account_transaction_settlement();
    let guest = application
        .world_mut()
        .spawn(Wallet(Money::from_cents(500)))
        .id();
    let operation = application.world_mut().spawn_empty().id();
    application.world_mut().write_message(TransactionRequest {
        domain:
            openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Simulation,
        operation,
        debit: Account::Entity(guest),
        credit: Account::Zoo,
        amount: Money::from_cents(125),
        kind: TransactionKind::Admission,
        subject: Some(guest),
    });

    application.update();

    assert_eq!(
        application.world().get::<Wallet>(guest),
        Some(&Wallet(Money::from_cents(375)))
    );
    assert_eq!(
        application
            .world()
            .get::<ZooCash>(zoo_root(&application))
            .unwrap()
            .0,
        Money::from_cents(1_125)
    );
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
    let guest = application
        .world_mut()
        .spawn(Wallet(Money::from_cents(20)))
        .id();
    let operation = application.world_mut().spawn_empty().id();
    application.world_mut().write_message(TransactionRequest {
        domain:
            openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Simulation,
        operation,
        debit: Account::Entity(guest),
        credit: Account::Zoo,
        amount: Money::from_cents(21),
        kind: TransactionKind::Purchase,
        subject: None,
    });

    application.update();

    assert_eq!(
        application.world().get::<Wallet>(guest),
        Some(&Wallet(Money::from_cents(20)))
    );
    assert_eq!(
        application
            .world()
            .get::<ZooCash>(zoo_root(&application))
            .unwrap()
            .0,
        Money::from_cents(1_000)
    );
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
    let guest = application
        .world_mut()
        .spawn(Wallet(Money::from_cents(100)))
        .id();
    for amount in [Money::from_cents(80), Money::from_cents(30)] {
        let operation = application.world_mut().spawn_empty().id();
        application.world_mut().write_message(TransactionRequest {
            domain: openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Simulation,
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
        Some(&Wallet(Money::from_dollars(1.0 - 0.8).unwrap()))
    );
    assert_eq!(
        application
            .world()
            .get::<ZooCash>(zoo_root(&application))
            .unwrap()
            .0,
        Money::from_cents(1_080)
    );
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
    let guest = application
        .world_mut()
        .spawn(Wallet(Money::from_cents(100)))
        .id();
    let operation = application.world_mut().spawn_empty().id();
    let request = TransactionRequest {
        domain:
            openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Simulation,
        operation,
        debit: Account::Entity(guest),
        credit: Account::Zoo,
        amount: Money::from_cents(25),
        kind: TransactionKind::Service,
        subject: Some(guest),
    };
    application.world_mut().write_message(request);
    application.world_mut().write_message(request);

    application.update();

    assert_eq!(
        application.world().get::<Wallet>(guest),
        Some(&Wallet(Money::from_cents(75)))
    );
    assert_eq!(
        application
            .world()
            .get::<ZooCash>(zoo_root(&application))
            .unwrap()
            .0,
        Money::from_cents(1_025)
    );
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
    use openzt2_world_state::simulation_time::simulation_clock_types::ZooMonth;
    use openzt2_zoo_state::economy::general_economy_types::{
        GeneralEconomyLedger, GeneralEconomyTransaction,
    };
    use openzt2_zoo_state::economy::monthly_finance_types::{
        FinanceMetric, FinancePeriod, FinanceValue,
    };

    let mut application = create_application_with_account_transaction_settlement();
    let root = zoo_root(&application);
    application.world_mut().entity_mut(root).insert((
        ZooCash(Money::from_cents(3_624_000)),
        ZooMonth(12),
        GeneralEconomyLedger::from_starting_balance(12, Money::from_cents(3_706_800)),
    ));
    let build = application
        .world_mut()
        .spawn(GeneralEconomyTransaction::Build)
        .id();
    let build_request = TransactionRequest {
        domain: openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Player,
        operation: build,
        debit: Account::Zoo,
        credit: Account::External,
        amount: Money::from_cents(1_000),
        kind: TransactionKind::Construction,
        subject: None,
    };
    application.world_mut().write_message(build_request);
    application.world_mut().write_message(build_request);
    application.update();

    let guest = application
        .world_mut()
        .spawn(Wallet(Money::from_cents(2_000)))
        .id();
    let service = application.world_mut().spawn_empty().id();
    application.world_mut().write_message(TransactionRequest {
        domain:
            openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Simulation,
        operation: service,
        debit: Account::Entity(guest),
        credit: Account::Zoo,
        amount: Money::from_cents(500),
        kind: TransactionKind::Service,
        subject: None,
    });
    let rejected = application
        .world_mut()
        .spawn(GeneralEconomyTransaction::Research)
        .id();
    application.world_mut().write_message(TransactionRequest {
        domain: openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Player,
        operation: rejected,
        debit: Account::Zoo,
        credit: Account::External,
        amount: Money::from_cents(4_000_000),
        kind: TransactionKind::Research,
        subject: None,
    });
    application.world_mut().write_message(build_request);
    application.update();
    assert_eq!(
        application
            .world()
            .get::<ZooCash>(zoo_root(&application))
            .unwrap()
            .0,
        Money::from_cents(3_623_500)
    );
    assert_eq!(
        application
            .world()
            .get::<GeneralEconomyLedger>(root)
            .unwrap()
            .cash(),
        Some(Money::from_cents(3_705_800))
    );

    application.world_mut().get_mut::<ZooMonth>(root).unwrap().0 = 13;
    let compensation = application
        .world_mut()
        .spawn(GeneralEconomyTransaction::Build)
        .id();
    application.world_mut().write_message(TransactionRequest {
        domain: openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Player,
        operation: compensation,
        debit: Account::External,
        credit: Account::Zoo,
        amount: Money::from_cents(1_000),
        kind: TransactionKind::Construction,
        subject: None,
    });
    application.update();
    let ledger = application
        .world()
        .get::<GeneralEconomyLedger>(root)
        .unwrap();
    assert_eq!(ledger.cash(), Some(Money::from_cents(3_706_800)));
    assert_eq!(ledger.history().retained_month_finance_records().len(), 2);
    assert_eq!(
        ledger
            .history()
            .finance_metric_value(FinanceMetric::Income, FinancePeriod::Lifetime),
        FinanceValue::Money(Money::from_cents(1_000))
    );
    assert_eq!(
        ledger
            .history()
            .finance_metric_value(FinanceMetric::Expenses, FinancePeriod::Lifetime),
        FinanceValue::Money(Money::from_cents(1_000))
    );
    assert_eq!(
        ledger
            .history()
            .finance_metric_value(FinanceMetric::TotalUsers, FinancePeriod::Lifetime),
        FinanceValue::Users(0)
    );
    assert!(ledger
        .history()
        .retained_month_finance_records()
        .all(|month| month.construction == Money::ZERO
            && month.active_users == 0
            && month.total_users == 0));

    let maximum = Money::from_dollars(f32::MAX).unwrap();
    application
        .world_mut()
        .entity_mut(root)
        .insert(GeneralEconomyLedger::from_starting_balance(13, maximum));
    let overflow = application
        .world_mut()
        .spawn(GeneralEconomyTransaction::Admission)
        .id();
    let zoo_before = application
        .world()
        .get::<ZooCash>(zoo_root(&application))
        .unwrap()
        .0;
    application.world_mut().write_message(TransactionRequest {
        domain:
            openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Simulation,
        operation: overflow,
        debit: Account::External,
        credit: Account::Zoo,
        amount: maximum,
        kind: TransactionKind::Admission,
        subject: None,
    });
    application.update();
    assert_eq!(
        application.world().get::<Wallet>(guest).unwrap().0,
        Money::from_cents(1_500)
    );
    assert_eq!(
        application
            .world()
            .get::<ZooCash>(zoo_root(&application))
            .unwrap()
            .0,
        zoo_before
    );
    assert_eq!(
        application
            .world()
            .get::<GeneralEconomyLedger>(root)
            .unwrap()
            .cash(),
        Some(maximum)
    );
}
