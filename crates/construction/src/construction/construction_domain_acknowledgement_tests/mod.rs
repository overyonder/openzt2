use bevy::prelude::*;

use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditApplicationAcknowledged;
use openzt2_zoo_state::shows::show_platform_upgrade_types::ShowPlatformUpgradeEditAcknowledged;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainEditAcknowledged;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditAcknowledged;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditApplicationAcknowledged;

use crate::construction::construction_domain_acknowledgement_collection;
use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionTransactionApplied;
use openzt2_zoo_state::construction::construction_edit_history_types::RecordAppliedConstructionTransaction;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitFailed;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitted;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionFeedbackOrigin;
use openzt2_zoo_state::construction::construction_transaction_types::DomainAckStatus;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitPhase;
use openzt2_zoo_state::construction::construction_transaction_types::EditCommitProgress;
use openzt2_zoo_state::construction::construction_transaction_types::EditTransaction;
use openzt2_zoo_state::construction::construction_transaction_types::RollbackContext;
use openzt2_zoo_state::construction::construction_transaction_types::TransactionState;

fn applying_initial_commit_progress_with_domain_statuses(
    statuses: [DomainAckStatus; 3],
) -> EditCommitProgress {
    EditCommitProgress {
        phase: EditCommitPhase::Applying,
        application: EditApplication::InitialCommit,
        cost: Money(500),
        terrain: statuses[0],
        topology: statuses[1],
        placement: statuses[2],
        shows: DomainAckStatus::NotExpected,
        aquatic: DomainAckStatus::NotExpected,
    }
}

fn construction_acknowledgement_test_application() -> App {
    let mut application = App::new();
    application
        .add_message::<openzt2_zoo_state::aquatic::tank_floor_edit_types::TankFloorEditAcknowledged>()
        .add_message::<TerrainEditAcknowledged>()
        .add_message::<TopologyEditAcknowledged>()
        .add_message::<ObjectPlacementEditApplicationAcknowledged>()
        .add_message::<ShowPlatformUpgradeEditAcknowledged>()
        .add_message::<TransportTrackConstructionEditApplicationAcknowledged>()
        .add_message::<RecordAppliedConstructionTransaction>()
        .add_message::<ConstructionTransactionApplied>()
        .add_message::<ConstructionCommitFailed>()
        .add_message::<ConstructionCommitted>()
        .add_systems(
            Update,
            construction_domain_acknowledgement_collection::collect_construction_domain_application_acknowledgements,
        );
    application
}

fn spawn_applying_initial_commit(
    application: &mut App,
    domain_statuses: [DomainAckStatus; 3],
) -> Entity {
    application
        .world_mut()
        .spawn(applying_initial_commit_progress_with_domain_statuses(
            domain_statuses,
        ))
        .id()
}

#[test]
fn multi_domain_commit_waits_for_every_expected_acknowledgement() {
    let mut application = construction_acknowledgement_test_application();
    let transaction_entity = spawn_applying_initial_commit(
        &mut application,
        [
            DomainAckStatus::Pending,
            DomainAckStatus::Pending,
            DomainAckStatus::NotExpected,
        ],
    );
    application
        .world_mut()
        .write_message(TerrainEditAcknowledged {
            transaction: transaction_entity,
            application: EditApplication::InitialCommit,
            accepted: true,
        });
    application.update();
    let edit_commit_progress = application
        .world()
        .get::<EditCommitProgress>(transaction_entity)
        .unwrap();
    assert_eq!(edit_commit_progress.phase, EditCommitPhase::Applying);
    assert_eq!(edit_commit_progress.terrain, DomainAckStatus::Applied);

    application
        .world_mut()
        .write_message(TopologyEditAcknowledged {
            transaction: transaction_entity,
            application: EditApplication::InitialCommit,
            accepted: true,
        });
    application.update();
    assert_eq!(
        application
            .world()
            .get::<EditCommitProgress>(transaction_entity)
            .unwrap()
            .phase,
        EditCommitPhase::Complete
    );
}

#[test]
fn stale_and_duplicate_acknowledgements_do_not_advance_another_domain() {
    let mut application = construction_acknowledgement_test_application();
    let transaction_entity = spawn_applying_initial_commit(
        &mut application,
        [
            DomainAckStatus::Pending,
            DomainAckStatus::Pending,
            DomainAckStatus::NotExpected,
        ],
    );
    for edit_application in [
        EditApplication::Undo,
        EditApplication::InitialCommit,
        EditApplication::InitialCommit,
    ] {
        application
            .world_mut()
            .write_message(TerrainEditAcknowledged {
                transaction: transaction_entity,
                application: edit_application,
                accepted: true,
            });
    }
    application.update();
    let edit_commit_progress = application
        .world()
        .get::<EditCommitProgress>(transaction_entity)
        .unwrap();
    assert_eq!(edit_commit_progress.terrain, DomainAckStatus::Applied);
    assert_eq!(edit_commit_progress.topology, DomainAckStatus::Pending);
    assert_eq!(edit_commit_progress.phase, EditCommitPhase::Applying);
}

#[test]
fn successful_initial_commit_emits_one_spend_fact_at_the_confirming_pointer() {
    let mut application = construction_acknowledgement_test_application();
    let transaction_entity = application
        .world_mut()
        .spawn((
            EditTransaction {
                sequence: 1,
                state: TransactionState::Applied,
                cost: Money(6_000),
            },
            applying_initial_commit_progress_with_domain_statuses([
                DomainAckStatus::NotExpected,
                DomainAckStatus::NotExpected,
                DomainAckStatus::Pending,
            ]),
            ConstructionFeedbackOrigin(Vec2::new(480.0, 270.0)),
        ))
        .id();
    for _ in 0..2 {
        application
            .world_mut()
            .write_message(ObjectPlacementEditApplicationAcknowledged {
                transaction: transaction_entity,
                application: EditApplication::InitialCommit,
                accepted: true,
            });
    }

    application.update();

    let construction_committed_messages = application
        .world()
        .resource::<Messages<ConstructionCommitted>>();
    let mut construction_committed_cursor = construction_committed_messages.get_cursor();
    let construction_committed = construction_committed_cursor
        .read(construction_committed_messages)
        .next()
        .unwrap();
    assert_eq!(construction_committed.transaction, transaction_entity);
    assert_eq!(construction_committed.cost, Money(6_000));
    assert_eq!(
        construction_committed.screen_position,
        Vec2::new(480.0, 270.0)
    );
    assert!(construction_committed_cursor
        .read(construction_committed_messages)
        .next()
        .is_none());
}

#[test]
fn partial_domain_failure_enters_typed_rollback() {
    let mut application = construction_acknowledgement_test_application();
    let transaction_entity = spawn_applying_initial_commit(
        &mut application,
        [
            DomainAckStatus::Pending,
            DomainAckStatus::Pending,
            DomainAckStatus::NotExpected,
        ],
    );
    application
        .world_mut()
        .write_message(TerrainEditAcknowledged {
            transaction: transaction_entity,
            application: EditApplication::InitialCommit,
            accepted: true,
        });
    application
        .world_mut()
        .write_message(TopologyEditAcknowledged {
            transaction: transaction_entity,
            application: EditApplication::InitialCommit,
            accepted: false,
        });
    application.update();
    let edit_commit_progress = application
        .world()
        .get::<EditCommitProgress>(transaction_entity)
        .unwrap();
    assert_eq!(edit_commit_progress.phase, EditCommitPhase::RollingBack);
    assert_eq!(
        edit_commit_progress.application,
        EditApplication::FailureRollback
    );
    assert_eq!(edit_commit_progress.terrain, DomainAckStatus::Applied);
    assert_eq!(edit_commit_progress.topology, DomainAckStatus::Reverted);
    assert_eq!(
        application
            .world()
            .get::<RollbackContext>(transaction_entity)
            .unwrap()
            .failed_application,
        EditApplication::InitialCommit
    );
}
