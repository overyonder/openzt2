use super::*;
use openzt2_zoo_state::{
    terrain::terrain_edit_types::{TerrainEdit, TerrainEditAcknowledged, TerrainEditPrepared},
    topology::topology_edit_types::{TopologyEdit, TopologyEditAcknowledged, TopologyEditPrepared},
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
    app.world_mut()
        .spawn((
            ConstructionActive,
            ApplyEdit(EditApplication::InitialCommit),
            edit(Money(500)),
            PreparedDomain::<TerrainEdit>::new(Money(400)),
            PreparedDomain::<TopologyEdit>::new(Money(100)),
            ConstructionFeedbackOrigin(Vec2::new(480.0, 270.0)),
        ))
        .id()
}

#[test]
fn multi_domain_cost_overflow_rejects_before_economy() {
    let mut app = fixture();
    let transaction = app.world_mut().spawn((Preparing, edit(Money(0)))).id();
    app.world_mut().write_message(TerrainEditPrepared {
        transaction,
        cost: Money(i64::MAX),
    });
    app.world_mut().write_message(TopologyEditPrepared {
        transaction,
        cost: Money(1),
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
    assert_eq!(events[0].cost, Money(500));
    assert_eq!(events[0].screen_position, Vec2::new(480.0, 270.0));
}

#[test]
fn rejection_waits_for_inflight_results_before_rolling_back_accepted_work() {
    let mut app = fixture();
    let transaction = applying(&mut app);
    app.world_mut().write_message(TopologyEditAcknowledged {
        transaction,
        application: EditApplication::InitialCommit,
        accepted: false,
    });
    step(&mut app);
    assert!(app.world().get::<RollbackContext>(transaction).is_none());
    app.world_mut().write_message(TerrainEditAcknowledged {
        transaction,
        application: EditApplication::InitialCommit,
        accepted: true,
    });
    step(&mut app);
    assert_eq!(
        app.world()
            .get::<RollbackContext>(transaction)
            .unwrap()
            .failed_application,
        EditApplication::InitialCommit,
    );
}
