use super::*;
use openzt2_zoo_state::{
    terrain::terrain_edit_types::{
        CommitTerrainEdit, TerrainEdit, TerrainEditAcknowledged, TerrainEditPrepared,
    },
    topology::topology_edit_types::{
        CommitTopologyEdit, TopologyEdit, TopologyEditAcknowledged, TopologyEditPrepared,
    },
};

fn fixture() -> App {
    let mut app = App::new();
    app.add_message::<ConstructionCommitFailed>()
        .add_message::<ConstructionCommitted>()
        .add_message::<ConstructionTransactionApplied>()
        .add_message::<RecordAppliedConstructionTransaction>()
        .add_message::<ApplyConstructionTransaction>()
        .add_message::<TransactionRequest>()
        .add_message::<TransactionCompleted>()
        .add_message::<TransactionRejected>()
        .configure_sets(
            FixedUpdate,
            (
                FixedConstructionSet::Prepare,
                FixedConstructionSet::RequestEconomy,
                FixedConstructionSet::CompleteEconomy,
                FixedConstructionSet::Apply,
                FixedConstructionSet::Cleanup,
            )
                .chain(),
        );
    install(&mut app);
    app
}

fn edit(cost: Money) -> EditTransaction {
    EditTransaction {
        sequence: 1,
        state: TransactionState::Applied,
        cost,
    }
}

fn step(app: &mut App) {
    app.world_mut().run_schedule(FixedUpdate);
}

fn applying(app: &mut App) -> Entity {
    let root = app.world_mut().spawn_empty().id();
    app.world_mut()
        .spawn((
            WorldMember { root },
            ConstructionActive,
            ApplyEdit(EditApplication::InitialCommit),
            edit(Money::from_cents(500)),
            PreparedDomain::<TerrainEdit>::new(Money::from_cents(400)),
            PreparedDomain::<TopologyEdit>::new(Money::from_cents(100)),
            ConstructionFeedbackOrigin(Vec2::new(480.0, 270.0)),
        ))
        .id()
}

#[test]
fn multi_domain_cost_overflow_rejects_before_economy() {
    let mut app = fixture();
    let transaction = app.world_mut().spawn((Preparing, edit(Money::ZERO))).id();
    app.world_mut().write_message(TerrainEditPrepared {
        transaction,
        cost: Money::from_dollars(f32::MAX).unwrap(),
    });
    app.world_mut().write_message(TopologyEditPrepared {
        transaction,
        cost: Money::from_dollars(f32::MAX).unwrap(),
    });
    step(&mut app);
    assert!(app.world().get_entity(transaction).is_err());
    assert!(app
        .world()
        .resource::<Messages<TransactionRequest>>()
        .is_empty());
}

#[test]
fn commit_waits_for_each_domain_and_reports_the_confirming_pointer_once() {
    let mut app = fixture();
    let transaction = applying(&mut app);
    for application in [
        EditApplication::Undo,
        EditApplication::InitialCommit,
        EditApplication::InitialCommit,
    ] {
        app.world_mut().write_message(TerrainEditAcknowledged {
            transaction,
            application,
            accepted: application == EditApplication::InitialCommit,
        });
    }
    step(&mut app);
    assert!(app.world().get::<ConstructionActive>(transaction).is_some());
    assert!(app
        .world()
        .resource::<Messages<ConstructionCommitted>>()
        .is_empty());

    app.world_mut().write_message(TerrainEditAcknowledged {
        transaction,
        application: EditApplication::InitialCommit,
        accepted: false,
    });
    app.world_mut().write_message(TopologyEditAcknowledged {
        transaction,
        application: EditApplication::Redo,
        accepted: true,
    });
    step(&mut app);
    assert!(app.world().get::<ConstructionActive>(transaction).is_some());
    assert!(app.world().get::<RollbackContext>(transaction).is_none());
    assert!(app
        .world()
        .resource::<Messages<ConstructionCommitted>>()
        .is_empty());

    app.world_mut().write_message(TopologyEditAcknowledged {
        transaction,
        application: EditApplication::InitialCommit,
        accepted: true,
    });
    step(&mut app);
    assert!(app.world().get::<ConstructionActive>(transaction).is_none());
    let committed = app.world().resource::<Messages<ConstructionCommitted>>();
    let mut cursor = committed.get_cursor();
    let events = cursor.read(committed).collect::<Vec<_>>();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].transaction, transaction);
    assert_eq!(events[0].cost, Money::from_cents(500));
    assert_eq!(events[0].screen_position, Vec2::new(480.0, 270.0));
}

#[derive(Resource, Default)]
struct AppliedTestTerrain(i32);

fn apply_and_revert_test_terrain(
    mut requests: MessageReader<CommitTerrainEdit>,
    mut results: MessageWriter<TerrainEditAcknowledged>,
    mut terrain: ResMut<AppliedTestTerrain>,
) {
    for request in requests.read() {
        terrain.0 += match request.application {
            EditApplication::InitialCommit => 1,
            EditApplication::FailureRollback => -1,
            application => panic!("unexpected terrain application {application:?}"),
        };
        results.write(TerrainEditAcknowledged {
            transaction: request.transaction,
            application: request.application,
            accepted: true,
        });
    }
}

#[test]
fn a_partial_commit_restores_the_applied_domain_and_refunds_the_charge() {
    let mut app = fixture();
    app.init_resource::<AppliedTestTerrain>().add_systems(
        FixedUpdate,
        apply_and_revert_test_terrain
            .in_set(FixedConstructionSet::Apply)
            .after(openzt2_zoo_state::construction::ConstructionDomainApplicationDispatchSet)
            .after(openzt2_zoo_state::construction::ConstructionFailureRollbackDispatchSet)
            .before(ConstructionDomainAcknowledgementCollectionSet),
    );
    let transaction = applying(&mut app);
    app.world_mut().entity_mut(transaction).insert((
        TerrainEdit {
            deltas: Vec::new(),
            water_regions: Vec::new(),
        },
        TopologyEdit {
            created: Box::default(),
            removed: Box::default(),
        },
    ));
    let mut topology_results = app
        .world()
        .resource::<Messages<CommitTopologyEdit>>()
        .get_cursor();
    step(&mut app);
    assert_eq!(app.world().resource::<AppliedTestTerrain>().0, 1);
    assert!(app.world().get::<RollbackContext>(transaction).is_none());
    let initial = topology_results
        .read(app.world().resource::<Messages<CommitTopologyEdit>>())
        .next()
        .copied()
        .unwrap();
    assert_eq!(initial.application, EditApplication::InitialCommit);

    app.world_mut().write_message(TopologyEditAcknowledged {
        transaction,
        application: EditApplication::InitialCommit,
        accepted: false,
    });
    step(&mut app);
    assert_eq!(app.world().resource::<AppliedTestTerrain>().0, 1);
    assert_eq!(
        app.world()
            .get::<RollbackContext>(transaction)
            .unwrap()
            .failed_application,
        EditApplication::InitialCommit,
    );
    step(&mut app);
    assert_eq!(app.world().resource::<AppliedTestTerrain>().0, 0);
    assert_eq!(
        topology_results
            .read(app.world().resource::<Messages<CommitTopologyEdit>>())
            .count(),
        0
    );
    step(&mut app);
    let payments = app.world().resource::<Messages<TransactionRequest>>();
    let mut payment_cursor = payments.get_cursor();
    let refund = payment_cursor.read(payments).next().copied().unwrap();
    assert_eq!(
        (refund.debit, refund.credit, refund.amount),
        (Account::External, Account::Zoo, Money::from_cents(500))
    );
    app.world_mut().write_message(TransactionCompleted {
        domain: openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Player,
        operation: refund.operation,
        debit: refund.debit,
        credit: refund.credit,
        amount: refund.amount,
        kind: refund.kind,
        subject: refund.subject,
    });
    step(&mut app);
    assert!(app.world().get_entity(transaction).is_err());
    assert!(app
        .world()
        .resource::<Messages<ConstructionCommitted>>()
        .is_empty());
    assert!(app
        .world()
        .resource::<Messages<RecordAppliedConstructionTransaction>>()
        .is_empty());
    let failures = app.world().resource::<Messages<ConstructionCommitFailed>>();
    let mut cursor = failures.get_cursor();
    let failures = cursor.read(failures).collect::<Vec<_>>();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].transaction, transaction);
    assert_eq!(
        failures[0].reason,
        ConstructionCommitFailure::DomainRejected
    );
}
