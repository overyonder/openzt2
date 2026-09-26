use super::*;
use crate::plugins::{
    staff::{
        staff_job_completion_and_cancellation::cancel_requested_jobs_and_jobs_with_invalid_world_relations,
        staff_lifecycle_messages::{CancelStaffJobRequest, StaffJobCancelled},
    },
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_game_data::{world_definitions::staff_management::StaffJobKind, AssetId};

#[test]
fn claimed_task_waits_for_its_asset_and_does_not_restart_after_terminal_execution() {
    use openzt2_game_data::behavior::document::{
        BehaviorDocument, BehaviorTask, BehaviorTaskActionPhase,
    };
    let mut app = App::new();
    app.init_resource::<Assets<BehaviorDocumentAsset>>()
        .init_resource::<BehaviorTaskExecutionSequence>()
        .add_message::<CancelStaffJobRequest>()
        .add_systems(
            Update,
            start_unstarted_authored_behavior_tasks_for_claimed_staff_jobs,
        );
    let document = app
        .world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .reserve_handle();
    let target = app.world_mut().spawn_empty().id();
    let worker = app.world_mut().spawn(Staff).id();
    let job = app
        .world_mut()
        .spawn((
            StaffJob {
                kind: StaffJobKind::Feed,
                target,
                urgency: 20,
                token: None,
            },
            JobClaim { staff: worker },
            StaffJobBehaviorTask {
                task: AssetId::from_key("keeper:fill"),
                document: document.clone(),
                declaration_index: 0,
                execution_id: None,
            },
        ))
        .id();
    app.world_mut()
        .entity_mut(worker)
        .insert(CurrentJob { job });
    for _ in 0..2 {
        app.update();
        assert!(app
            .world()
            .get::<BehaviorTaskExecutionState>(worker)
            .is_none());
        assert!(app
            .world()
            .resource::<Messages<CancelStaffJobRequest>>()
            .is_empty());
    }
    let task = BehaviorTask {
        id: AssetId::from_key("keeper:fill"),
        name: "keeper:fill".into(),
        priority: None,
        reservation_tag: None,
        task_delay_seconds: None,
        subjects: vec![],
        targets: vec![],
        objects: vec![],
        candidate_eligibility_requirements: vec![],
        scores: vec![],
        execution: BehaviorTaskActionPhase::Supported(vec![]),
        completion: BehaviorTaskActionPhase::Supported(vec![]),
        failure: BehaviorTaskActionPhase::Supported(vec![]),
    };
    let unrelated = BehaviorTask {
        id: AssetId::from_key("keeper:other"),
        name: "keeper:other".into(),
        ..task.clone()
    };
    app.world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .insert(
            document.id(),
            BehaviorDocumentAsset::from_test_document(BehaviorDocument::Tasks(vec![
                unrelated,
                task.clone(),
                task,
            ])),
        )
        .unwrap();
    app.update();
    let executing = app
        .world()
        .get::<BehaviorTaskExecutionState>(worker)
        .unwrap();
    assert_eq!(executing.target, Some(target));
    assert_eq!(executing.document, document);
    assert_eq!(executing.program, AssetId::from_key("keeper:fill"));
    assert_eq!(executing.declaration, 2);
    assert!(app
        .world()
        .get::<StaffJobBehaviorTask>(job)
        .unwrap()
        .execution_id
        .is_some());
    app.world_mut()
        .entity_mut(worker)
        .remove::<BehaviorTaskExecutionState>();
    app.update();
    assert!(app
        .world()
        .get::<BehaviorTaskExecutionState>(worker)
        .is_none());
    assert!(app
        .world()
        .resource::<Messages<CancelStaffJobRequest>>()
        .is_empty());
}

#[test]
fn removed_source_task_releases_assignment_without_cancelling_live_request() {
    use openzt2_game_data::behavior::document::BehaviorDocument;
    let mut app = App::new();
    app.init_resource::<Assets<BehaviorDocumentAsset>>()
        .init_resource::<BehaviorTaskExecutionSequence>()
        .add_message::<CancelStaffJobRequest>()
        .add_systems(
            Update,
            start_unstarted_authored_behavior_tasks_for_claimed_staff_jobs,
        );
    let document = app
        .world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .add(BehaviorDocumentAsset::from_test_document(
            BehaviorDocument::Tasks(vec![]),
        ));
    let target = app.world_mut().spawn_empty().id();
    let worker = app.world_mut().spawn(Staff).id();
    let job = app
        .world_mut()
        .spawn((
            StaffJob {
                kind: StaffJobKind::Feed,
                target,
                urgency: 20,
                token: Some(AssetId::from_key("t_fillfoodcontainer")),
            },
            JobClaim { staff: worker },
            StaffJobBehaviorTask {
                document,
                task: AssetId::from_key("keeper:removed"),
                declaration_index: 0,
                execution_id: None,
            },
        ))
        .id();
    app.world_mut()
        .entity_mut(worker)
        .insert(CurrentJob { job });
    app.update();
    assert!(app.world().get::<StaffJob>(job).is_some());
    assert!(app.world().get::<JobClaim>(job).is_none());
    assert!(app.world().get::<StaffJobBehaviorTask>(job).is_none());
    assert!(app.world().get::<CurrentJob>(worker).is_none());
    assert!(app.world().get::<AvailableForWork>(worker).is_some());
    assert!(app
        .world()
        .get::<BehaviorTaskExecutionState>(worker)
        .is_none());
    assert!(app
        .world()
        .resource::<Messages<CancelStaffJobRequest>>()
        .is_empty());
}

#[test]
fn failed_assignment_preserves_request_until_its_owner_cancels_it() {
    let mut app = App::new();
    app.add_message::<BehaviorTaskFailed>()
        .add_message::<CancelStaffJobRequest>()
        .add_message::<StaffJobCancelled>()
        .add_systems(
            Update,
            (
                release_staff_assignment_after_authored_behavior_failure_finishes,
                cancel_requested_jobs_and_jobs_with_invalid_world_relations,
            )
                .chain(),
        );
    let root = app.world_mut().spawn_empty().id();
    let target = app.world_mut().spawn(WorldMember { root }).id();
    let worker = app.world_mut().spawn(Staff).id();
    let request = app
        .world_mut()
        .spawn((
            StaffJob {
                kind: StaffJobKind::Feed,
                target,
                urgency: 20,
                token: Some(AssetId::from_key("t_fillfoodcontainer")),
            },
            JobClaim { staff: worker },
            StaffJobBehaviorTask {
                task: AssetId::from_key("keeper:fill"),
                document: Handle::default(),
                declaration_index: 0,
                execution_id: Some(1),
            },
        ))
        .id();
    app.world_mut()
        .entity_mut(worker)
        .insert(CurrentJob { job: request });
    app.world_mut().write_message(BehaviorTaskFailed {
        actor: worker,
        execution_id: 1,
    });
    app.update();
    assert!(app.world().get::<StaffJob>(request).is_some());
    assert!(app.world().get::<JobClaim>(request).is_none());
    assert!(app.world().get::<CurrentJob>(worker).is_none());
    assert!(app.world().get::<AvailableForWork>(worker).is_some());
    assert!(app
        .world()
        .get::<UnsuccessfulStaffJobCandidates>(request)
        .unwrap()
        .contains(worker));
    // No new quantity threshold crossing or synthetic request is required.
    app.update();
    assert!(app.world().get::<StaffJob>(request).is_some());
    app.world_mut()
        .write_message(CancelStaffJobRequest { job: request });
    app.update();
    assert!(app.world().get_entity(request).is_err());
}

#[test]
fn replaced_assignment_ignores_previous_execution_success_and_failure() {
    let mut app = App::new();
    app.add_message::<BehaviorTaskFailed>()
        .add_message::<BehaviorTaskFinished>()
        .add_systems(
            Update,
            (
                release_staff_assignment_after_authored_behavior_failure_finishes,
                settle_staff_jobs_after_authored_behavior_finishes,
            )
                .chain(),
        );
    let worker = app.world_mut().spawn(Staff).id();
    let job = app
        .world_mut()
        .spawn((
            StaffJob {
                kind: StaffJobKind::Treat,
                target: worker,
                urgency: 1,
                token: None,
            },
            JobClaim { staff: worker },
            StaffJobBehaviorTask {
                task: AssetId::from_key("keeper:fill"),
                document: Handle::default(),
                declaration_index: 0,
                execution_id: Some(2),
            },
        ))
        .id();
    app.world_mut()
        .entity_mut(worker)
        .insert(CurrentJob { job });
    app.world_mut().write_message(BehaviorTaskFailed {
        actor: worker,
        execution_id: 1,
    });
    app.world_mut().write_message(BehaviorTaskFinished {
        actor: worker,
        execution_id: 1,
    });
    app.update();
    assert!(app.world().get::<JobProgress>(job).is_none());
    assert!(app
        .world()
        .get::<UnsuccessfulStaffJobCandidates>(job)
        .is_none());
    assert_eq!(
        app.world().get::<CurrentJob>(worker),
        Some(&CurrentJob { job })
    );
    assert_eq!(
        app.world().get::<JobClaim>(job),
        Some(&JobClaim { staff: worker })
    );
    app.world_mut().write_message(BehaviorTaskFinished {
        actor: worker,
        execution_id: 2,
    });
    app.update();
    assert!(app.world().get::<JobProgress>(job).is_some());
}

#[test]
fn successful_container_refill_releases_worker_and_preserves_controller_request() {
    let mut app = App::new();
    app.add_message::<BehaviorTaskFinished>()
        .add_systems(Update, settle_staff_jobs_after_authored_behavior_finishes);
    let worker = app.world_mut().spawn(Staff).id();
    let target = app.world_mut().spawn_empty().id();
    let job = app
        .world_mut()
        .spawn((
            StaffJob {
                kind: StaffJobKind::Feed,
                target,
                urgency: 1,
                token: Some(AssetId::from_key("t_fillfoodcontainer")),
            },
            JobClaim { staff: worker },
            StaffJobBehaviorTask {
                task: AssetId::from_key("keeper:fill"),
                document: Handle::default(),
                declaration_index: 0,
                execution_id: Some(1),
            },
        ))
        .id();
    app.world_mut()
        .entity_mut(worker)
        .insert(CurrentJob { job });
    app.world_mut().write_message(BehaviorTaskFinished {
        actor: worker,
        execution_id: 1,
    });
    app.update();
    assert!(app.world().get::<StaffJob>(job).is_some());
    assert!(app.world().get::<JobClaim>(job).is_none());
    assert!(app.world().get::<StaffJobBehaviorTask>(job).is_none());
    assert!(app.world().get::<JobProgress>(job).is_none());
    assert!(app.world().get::<CurrentJob>(worker).is_none());
    assert!(app.world().get::<AvailableForWork>(worker).is_some());
}
