use bevy::prelude::*;
use openzt2_game_data::{world_scenario::StartingZooRecord, AssetId};

use openzt2_simulation_time::simulation_time_world_lifecycle::initialize_simulation_time_from_selected_world;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_world_state::game_session_types::WorldSessionMode;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooCalendar;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::simulation_time::simulation_control_types::SimulationControl;

use crate::persistence::{
    persistence_failure_types::{WorldSnapshotPersistenceFailed, WorldSnapshotPersistenceFailure},
    slot_io::{
        pending_snapshot_application_types::PendingSnapshotApplication,
        photo_baseline_snapshot_tests::{
            initialize_fixture_album_localization, loading_app, photo_fixture_catalogue,
            prepare_photo_snapshot_application, prepare_unvalidated_snapshot_application,
            saved_photo_container_for_scenario, PROFILE, SCENARIO,
        },
    },
};
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    world_scenario::{
        world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios,
        world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
    },
};

use openzt2_world::world_spawn::world_hydration_completion::complete_or_fail_finished_world_hydration;
use openzt2_world::world_spawn::world_load_request_acceptance::{
    accept_queued_world_load_after_required_assets_resolve, admit_first_world_load_request,
};
use openzt2_world::world_spawn::world_unloading::unload_requested_worlds_and_their_members;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_hydration_completion::WorldLoadFinished;
use openzt2_world_state::world_spawn::world_hydration_types::WorldHydration;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompletionSet;
use openzt2_world_state::world_spawn::world_load_completion_marker::{
    ImportedPersistentIdsReserved, StartingFenceTopologyHydrated, StartingPathsHydrated,
    StartingSpawnValuesApplied, StartingTerrainHydrated, WorldLoadCompleted,
};
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailed;
use openzt2_world_state::world_spawn::world_load_failure::WorldLoadFailure;
use openzt2_world_state::world_spawn::world_load_request_acceptance::BeginWorldLoad;
use openzt2_world_state::world_spawn::world_load_request_acceptance::WorldLoadOperationOwner;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_world_state::world_spawn::world_unloading::UnloadWorld;

fn recovery_app() -> (App, Entity) {
    let (mut app, root) = loading_app();
    app.add_systems(
        Update,
        (
            unload_requested_worlds_and_their_members,
            admit_first_world_load_request.run_if(in_state(GamePhase::Loading)),
            accept_queued_world_load_after_required_assets_resolve
                .run_if(in_state(GamePhase::Loading)),
            initialize_simulation_time_from_selected_world.run_if(in_state(GamePhase::Loading)),
            complete_or_fail_finished_world_hydration
                .in_set(WorldLoadCompletionSet)
                .run_if(in_state(GamePhase::Loading)),
        )
            .chain()
            .in_set(GameSet::Intent),
    );
    (app, root)
}

fn fixture_start_document() -> WorldScenarioDocumentAsset {
    let mut document = photo_fixture_catalogue();
    document.starting_zoos.push(StartingZooRecord {
        next_persistent_identifier: None,
        environment: SCENARIO,
        id: SCENARIO,
        profile: PROFILE,
        map: SCENARIO,
        camera_position_m: [0.; 3],
        camera_rotation_radians: [0.; 3],
        name: "baseline".into(),
        admissions_open: true,
        cash_cents: 5000,
        general_cash_cents: None,
        admission_cents: Some(250),
        fame_half_stars: 0,
        maximum_fame_percent_reached: None,
        start_tick: 1,
        absolute_day: 0,
        calendar: [2000, 1, 1],
        imported_seed: Some(17),
        entities: vec![],
        spawn_values: vec![],
        paths: vec![],
        topology_nodes: vec![],
        fences: vec![],
    });
    WorldScenarioDocumentAsset::from_test_document_with_terrain_dependency(
        document,
        SCENARIO,
        "fixture/terrain.dat",
    )
}

fn install_start_document(app: &mut App) -> Handle<WorldScenarioDocumentAsset> {
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WorldScenarioDocumentAsset>>()
        .add(fixture_start_document());
    app.world_mut()
        .resource_mut::<WorldScenarios>()
        .queue_test_document(handle.clone());
    handle
}

fn begin_normal_reentry(app: &mut App) -> Entity {
    app.world_mut().write_message(BeginWorldLoad {
        saved_world_document: None,
        operation: None,
        scenario: SCENARIO,
        mode: WorldSessionMode::Freeform,
        profile: PROFILE,
        starting_cash_cents: None,
    });
    app.world_mut()
        .resource_mut::<NextState<GamePhase>>()
        .set(GamePhase::Loading);
    app.update();
    assert_eq!(
        *app.world().resource::<State<GamePhase>>().get(),
        GamePhase::Loading
    );
    let root = app
        .world_mut()
        .query_filtered::<Entity, With<WorldRoot>>()
        .single(app.world())
        .unwrap();
    assert!(app.world().get::<PersistentIdAllocator>(root).is_some());
    assert!(app.world().get::<WorldHydration>(root).is_some());
    assert!(app.world().get::<WorldLoadOperationOwner>(root).is_none());
    assert!(app.world().get::<ZooClock>(root).is_some());
    root
}

#[test]
fn post_baseline_reference_failure_retires_its_world_and_allows_normal_reentry() {
    let (mut app, root) = recovery_app();
    let _start = install_start_document(&mut app);
    initialize_fixture_album_localization(app.world_mut());
    prepare_photo_snapshot_application(app.world_mut());
    let operation = app
        .world_mut()
        .query_filtered::<Entity, With<PendingSnapshotApplication>>()
        .single(app.world())
        .unwrap();
    let member = app.world_mut().spawn(WorldMember { root }).id();
    let unrelated = app.world_mut().spawn_empty().id();
    // Geometry and imported-ID reservation have reached the completion boundary.
    // The completion owner still publishes the actual root and operation.
    app.world_mut()
        .entity_mut(root)
        .remove::<WorldLoadCompleted>()
        .insert((
            WorldHydration::new_pending_world_hydration(0),
            ZooClock {
                tick: 0,
                fractional_tick: Default::default(),
                absolute_day: 0,
                tick_in_day: 0,
            },
            ImportedPersistentIdsReserved,
            StartingTerrainHydrated,
            StartingFenceTopologyHydrated,
            StartingPathsHydrated,
            StartingSpawnValuesApplied,
        ));
    app.update();
    if matches!(
        app.world().resource::<NextState<GamePhase>>(),
        NextState::Unchanged
    ) {
        app.update();
    }
    // The checksum-valid snapshot contains a second challenge absent from the
    // loaded catalogue. Its real reference-validation failure requests cleanup.
    let failures = app
        .world()
        .resource::<Messages<WorldSnapshotPersistenceFailed>>();
    let mut cursor = failures.get_cursor();
    assert_eq!(
        cursor.read(failures).next().unwrap().failure_reason,
        WorldSnapshotPersistenceFailure::BrokenPersistentEntityReference
    );
    assert_eq!(
        *app.world().resource::<State<GamePhase>>().get(),
        GamePhase::Loading
    );
    assert!(matches!(
        app.world().resource::<NextState<GamePhase>>(),
        NextState::Pending(GamePhase::MapSelection)
    ));
    app.update();
    assert_eq!(
        *app.world().resource::<State<GamePhase>>().get(),
        GamePhase::MapSelection
    );
    assert!(app.world().get_entity(root).is_err());
    assert!(app.world().get_entity(member).is_err());
    assert!(app.world().get_entity(unrelated).is_ok());
    assert!(app.world().get::<PersistentIdAllocator>(root).is_none());
    assert!(app
        .world()
        .try_query::<&ZooClock>()
        .is_none_or(|mut state| state.iter(app.world()).next().is_none()));
    assert!(app
        .world()
        .try_query::<&ZooCalendar>()
        .is_none_or(|mut state| state.iter(app.world()).next().is_none()));
    assert!(app
        .world()
        .try_query::<&SimulationControl>()
        .is_none_or(|mut state| state.iter(app.world()).next().is_none()));
    assert!(app
        .world()
        .try_query::<&ZooSeed>()
        .is_none_or(|mut state| state.iter(app.world()).next().is_none()));
    assert!(app.world().get_entity(operation).is_err());
    let replacement = begin_normal_reentry(&mut app);
    assert_ne!(replacement, root);
}

#[test]
fn checksum_valid_unknown_scenario_fails_instead_of_retaining_a_pending_snapshot() {
    let (mut app, old_root) = recovery_app();
    let _start = install_start_document(&mut app);
    let unknown = AssetId([99; 16]);
    let bytes = saved_photo_container_for_scenario(app.world_mut(), unknown);
    let operation = prepare_unvalidated_snapshot_application(app.world_mut(), bytes);
    app.update(); // Validation precedes retirement of the existing world.
    assert!(app.world().get_entity(old_root).is_ok());
    app.update(); // Retire the old root and request the validated baseline.
    app.update(); // The settled catalogue cannot resolve this scenario.
    let failures = app.world().resource::<Messages<WorldLoadFailed>>();
    let mut cursor = failures.get_cursor();
    let failure = cursor.read(failures).next().unwrap();
    assert_eq!(failure.operation, Some(operation));
    assert_eq!(failure.scenario, unknown);
    assert_eq!(failure.root, None);
    assert_eq!(failure.reason, WorldLoadFailure::MissingScenario);
    assert!(app.world().get_entity(operation).is_err());
    app.update();
    assert_eq!(
        *app.world().resource::<State<GamePhase>>().get(),
        GamePhase::MapSelection
    );
    begin_normal_reentry(&mut app);
}

#[test]
fn pending_catalogue_waits_then_accepts_the_same_ordinary_request() {
    let (mut app, root) = recovery_app();
    app.world_mut().write_message(UnloadWorld(root));
    let handle = app
        .world()
        .resource::<Assets<WorldScenarioDocumentAsset>>()
        .reserve_handle();
    app.world_mut()
        .resource_mut::<WorldScenarios>()
        .queue_test_document(handle.clone());
    app.world_mut().write_message(BeginWorldLoad {
        saved_world_document: None,
        operation: None,
        scenario: SCENARIO,
        mode: WorldSessionMode::Freeform,
        profile: PROFILE,
        starting_cash_cents: None,
    });
    app.update();
    app.update();
    assert!(app
        .world()
        .resource::<Messages<WorldLoadFailed>>()
        .is_empty());
    assert!(app
        .world_mut()
        .query_filtered::<Entity, With<WorldRoot>>()
        .iter(app.world())
        .next()
        .is_none());
    app.world_mut()
        .resource_mut::<Assets<WorldScenarioDocumentAsset>>()
        .insert(handle.id(), fixture_start_document())
        .unwrap();
    app.update();
    app.update();
    let replacement = app
        .world_mut()
        .query_filtered::<Entity, With<WorldRoot>>()
        .single(app.world())
        .unwrap();
    assert!(app
        .world()
        .get::<PersistentIdAllocator>(replacement)
        .is_some());
    assert!(app.world().get::<WorldHydration>(replacement).is_some());
    assert!(app
        .world()
        .resource::<Messages<WorldLoadFailed>>()
        .is_empty());
}

#[test]
fn hydration_failure_retires_simulation_before_the_snapshot_coordinator_observes_it() {
    let (mut app, root) = recovery_app();
    let _handle = install_start_document(&mut app);
    prepare_photo_snapshot_application(app.world_mut());
    let operation = app
        .world_mut()
        .query_filtered::<Entity, With<PendingSnapshotApplication>>()
        .single(app.world())
        .unwrap();
    app.world_mut().entity_mut(root).insert((
        WorldHydration::new_pending_world_hydration(0),
        ZooClock {
            tick: 0,
            fractional_tick: Default::default(),
            absolute_day: 0,
            tick_in_day: 0,
        },
        WorldLoadFailure::MissingDependency(AssetId([77; 16])),
    ));
    app.update();
    assert!(app.world().get_entity(root).is_err());
    assert!(app.world().get_entity(operation).is_err());
    assert!(app.world().get::<PersistentIdAllocator>(root).is_none());
    assert!(app
        .world()
        .try_query::<&ZooClock>()
        .is_none_or(|mut state| state.iter(app.world()).next().is_none()));
    assert!(app
        .world()
        .try_query::<&ZooCalendar>()
        .is_none_or(|mut state| state.iter(app.world()).next().is_none()));
    assert!(app
        .world()
        .try_query::<&SimulationControl>()
        .is_none_or(|mut state| state.iter(app.world()).next().is_none()));
    assert!(app
        .world()
        .try_query::<&ZooSeed>()
        .is_none_or(|mut state| state.iter(app.world()).next().is_none()));
    assert_eq!(
        *app.world().resource::<State<GamePhase>>().get(),
        GamePhase::Loading
    );
    assert!(app.world().resource::<Messages<UnloadWorld>>().is_empty());
    assert!(matches!(
        app.world().resource::<NextState<GamePhase>>(),
        NextState::Pending(GamePhase::MapSelection)
    ));
    app.update();
    assert_eq!(
        *app.world().resource::<State<GamePhase>>().get(),
        GamePhase::MapSelection
    );
    assert!(app
        .world()
        .try_query::<&ZooClock>()
        .is_none_or(|mut state| state.iter(app.world()).next().is_none()));
    begin_normal_reentry(&mut app);
}

#[test]
fn stale_same_scenario_results_do_not_complete_or_fail_a_later_snapshot_operation() {
    let (mut app, root) = recovery_app();
    let _start = install_start_document(&mut app);
    prepare_photo_snapshot_application(app.world_mut());
    let retired = app.world_mut().spawn_empty().id();
    app.world_mut().entity_mut(retired).despawn();
    app.world_mut().write_message(WorldLoadFinished {
        operation: Some(retired),
        scenario: SCENARIO,
        root,
    });
    app.world_mut().write_message(WorldLoadFailed {
        operation: Some(retired),
        root: None,
        scenario: SCENARIO,
        reason: WorldLoadFailure::MissingScenario,
    });
    app.update();
    app.update();
    assert_eq!(
        *app.world().resource::<State<GamePhase>>().get(),
        GamePhase::Loading
    );
    assert!(matches!(
        app.world().resource::<NextState<GamePhase>>(),
        NextState::Unchanged
    ));
    assert!(app
        .world_mut()
        .query_filtered::<Entity, With<PendingSnapshotApplication>>()
        .single(app.world())
        .is_ok());
    assert!(app.world().get_entity(root).is_ok());
    assert!(app.world().get::<PersistentIdAllocator>(root).is_some());
    assert!(app
        .world()
        .resource::<Messages<WorldSnapshotPersistenceFailed>>()
        .is_empty());
    assert!(app.world().resource::<Messages<UnloadWorld>>().is_empty());
}
