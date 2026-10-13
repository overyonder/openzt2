use super::*;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::staff::staff_employment_types::AvailableForWork;
use openzt2_zoo_state::staff::staff_lifecycle_messages::StaffJobCancelled;

fn capture_app() -> (App, CaptureAnimalRequest) {
    let mut app = App::new();
    app.add_message::<CaptureAnimalRequest>()
        .add_message::<AnimalCaptured>()
        .add_message::<CancelStaffJobRequest>()
        .add_message::<StaffJobCancelled>();
    let root = app.world_mut().spawn_empty().id();
    let animal = app
        .world_mut()
        .spawn((
            Animal,
            WorldMember { root },
            Escaped { since_tick: 7 },
            Rampaging { elapsed_ticks: 4 },
            Tranquilized {
                remaining_ticks: 20,
                recovery_ticks: 5,
            },
        ))
        .id();
    let staff = app.world_mut().spawn(Staff).id();
    let job = app
        .world_mut()
        .spawn((
            StaffJob {
                kind: StaffJobKind::Capture,
                target: animal,
                urgency: 0,
                token: Some(AssetId::from_key("t_crateanimal")),
            },
            JobClaim { staff },
            JobProgress,
            StaffJobBehaviorTask {
                document: Handle::default(),
                task: AssetId::from_key("keeper:crateanimal"),
                declaration_index: 0,
                execution_id: Some(3),
            },
        ))
        .id();
    app.world_mut().entity_mut(staff).insert(CurrentJob { job });
    let request = CaptureAnimalRequest {
        animal,
        staff,
        job,
        execution_id: 3,
    };
    app.world_mut().write_message(request);
    (app, request)
}

#[test]
fn cancelled_capture_preserves_emergency_state_and_releases_claim_in_either_system_order() {
    for cancel_first in [false, true] {
        let (mut app, request) = capture_app();
        app.world_mut()
            .write_message(CancelStaffJobRequest { job: request.job });
        if cancel_first {
            app.add_systems(
                Update,
                (
                    cancel_requested_jobs_and_jobs_with_invalid_world_relations,
                    validate_staff_capture_requests_and_clear_animal_emergency_state,
                )
                    .chain(),
            );
        } else {
            app.add_systems(
                Update,
                (
                    validate_staff_capture_requests_and_clear_animal_emergency_state,
                    cancel_requested_jobs_and_jobs_with_invalid_world_relations,
                )
                    .chain(),
            );
        }
        app.update();
        assert!(app.world().get::<Escaped>(request.animal).is_some());
        assert!(app.world().get::<Rampaging>(request.animal).is_some());
        assert_eq!(
            app.world()
                .get::<Tranquilized>(request.animal)
                .unwrap()
                .remaining_ticks,
            20
        );
        assert!(app.world().get_entity(request.job).is_err());
        assert!(app.world().get::<CurrentJob>(request.staff).is_none());
        assert!(app.world().get::<AvailableForWork>(request.staff).is_some());
        assert!(app
            .world()
            .resource::<Messages<AnimalCaptured>>()
            .is_empty());
    }
}

use openzt2_zoo_staff::staff::staff_job_completion_and_cancellation::cancel_requested_jobs_and_jobs_with_invalid_world_relations;
