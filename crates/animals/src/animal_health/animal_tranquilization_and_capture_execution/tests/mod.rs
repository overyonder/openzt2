use super::*;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::staff::staff_lifecycle_messages::StaffJobCancelled;

#[test]
fn a_dose_batch_keeps_the_longest_sedation_and_last_recovery() {
    use openzt2_game_data::world_definitions::{
        animal_health::{TranquilizerDefinition, TranquilizerEligibility, TranquilizerModePolicy},
        document::WorldDefinitionDocument,
    };

    let long = AssetId::from_key("long-dose");
    let short = AssetId::from_key("short-dose");
    let document = WorldDefinitionDocument {
        tranquilizer_mode: Some(TranquilizerModePolicy {
            range_cm: 500,
            charge_points_per_second: 0.0,
            charge_decrease_points_per_second: 0.0,
            required_charge_points: 0.0,
            use_distance_cm: 0,
            shot_cue: AssetId::default(),
            misfire_cue: AssetId::default(),
            begin_charge_cue: AssetId::default(),
            charge_cue: AssetId::default(),
            end_charge_cue: AssetId::default(),
            lose_tracking_cue: AssetId::default(),
            shot_effect: AssetId::default(),
            shot_effect_distance_cm: 0,
            shot_effect_offset_cm: [0; 2],
            recoil_duration_seconds: 0.0,
            misfire_duration_seconds: 0.0,
            reticle_charging: AssetId::default(),
            reticle_ready: AssetId::default(),
        }),
        tranquilizers: [(long, 120, 15), (short, 30, 5)]
            .map(
                |(id, duration_ticks, recovery_ticks)| TranquilizerDefinition {
                    id,
                    duration_ticks,
                    recovery_ticks,
                    eligible_states: TranquilizerEligibility::CONTAINED,
                },
            )
            .to_vec(),
        ..default()
    };
    let mut assets = Assets::<WorldDefinitionAsset>::default();
    let handle = assets.add(WorldDefinitionAsset::from_test_document(document.clone()));
    let mut definitions = WorldDefinitions::default();
    definitions.index_test_document(handle, &document);
    let mut app = App::new();
    let step = Duration::from_millis(10);
    app.insert_resource(Time::<Fixed>::from_duration(step))
        .insert_resource(assets)
        .insert_resource(definitions)
        .add_message::<TranquilizeRequest>()
        .add_systems(
            Update,
            validate_tranquilizer_requests_and_apply_timed_tranquilization,
        );
    let source = app
        .world_mut()
        .spawn((ZooCamera, GlobalTransform::IDENTITY))
        .id();
    let transform = GlobalTransform::from_translation(Vec3::X * 5.0);
    let untranquilized = app.world_mut().spawn((Animal, transform)).id();
    let tranquilized = app
        .world_mut()
        .spawn((
            Animal,
            transform,
            Tranquilized {
                recovery_duration: step * 10,
            },
            TranquilizerDoseTimer(Timer::new(step * 60, TimerMode::Once)),
        ))
        .id();
    for animal in [untranquilized, tranquilized] {
        for tranquilizer in [long, short] {
            app.world_mut().write_message(TranquilizeRequest {
                animal,
                tranquilizer,
                source,
            });
        }
    }
    app.update();
    for animal in [untranquilized, tranquilized] {
        assert_eq!(
            app.world().get::<Tranquilized>(animal),
            Some(&Tranquilized {
                recovery_duration: step * 5
            })
        );
        assert_eq!(
            app.world()
                .get::<TranquilizerDoseTimer>(animal)
                .unwrap()
                .0
                .remaining(),
            step * 120
        );
    }
}

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
            Rampaging,
            Tranquilized {
                recovery_duration: Duration::from_millis(50),
            },
        ))
        .id();
    let staff = app.world_mut().spawn(Staff).id();
    let job = app
        .world_mut()
        .spawn((
            StaffJob {
                kind: Some(StaffJobKind::Capture),
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
        assert!(
            app.world()
                .resource::<Messages<AnimalCaptured>>()
                .is_empty()
        );
    }
}

use openzt2_zoo_staff::staff::staff_job_completion_and_cancellation::cancel_requested_jobs_and_jobs_with_invalid_world_relations;
