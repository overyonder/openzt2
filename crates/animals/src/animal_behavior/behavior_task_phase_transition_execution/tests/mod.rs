use super::*;
use arrayvec::ArrayVec;
use openzt2_game_data::{
    behavior::document::{BehaviorDocument, BehaviorSet, BehaviorTaskActionPhase},
    AssetId,
};

#[derive(Component)]
struct SelectionAttempts {
    target: Entity,
    accepted_updates: Vec<u64>,
}

fn record_guest_selection_attempts(
    clock: Res<ZooClock>,
    mut actors: Query<
        (&BehaviorTargetRetryDelays, &mut SelectionAttempts),
        With<openzt2_zoo_state::guests::guest_simulation_types::Guest>,
    >,
) {
    for (delays, mut attempts) in &mut actors {
        if !delays.rejects(attempts.target) {
            attempts.accepted_updates.push(clock.tick);
        }
    }
}

fn record_staff_selection_attempts(
    clock: Res<ZooClock>,
    mut actors: Query<
        (&BehaviorTargetRetryDelays, &mut SelectionAttempts),
        With<openzt2_zoo_state::staff::staff_employment_types::Staff>,
    >,
) {
    for (delays, mut attempts) in &mut actors {
        if !delays.rejects(attempts.target) {
            attempts.accepted_updates.push(clock.tick);
        }
    }
}

#[test]
fn guest_and_staff_selection_share_the_window_on_opposite_sides_of_behavior_updates() {
    use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;
    use openzt2_world_state::application_schedule::FixedGameSet;
    use openzt2_zoo_state::guests::guest_simulation_types::Guest;
    use openzt2_zoo_state::staff::staff_employment_types::Staff;

    let policy = BehaviorSelectionPolicy {
        enabled: true,
        minimum_score: 0.0,
        outside_range_need_value: 0.0,
        rejected_target_retry_interval: 50,
        rejected_target_retry_interval_cap: 2000,
    };
    let document = WorldDefinitionDocument {
        behavior_selection_policy: Some(policy),
        ..default()
    };
    let mut assets = Assets::<WorldDefinitionAsset>::default();
    let handle = assets.add(WorldDefinitionAsset::from_test_document(document.clone()));
    let mut definitions = WorldDefinitions::default();
    definitions.index_test_document(handle, &document);
    let mut app = App::new();
    app.insert_resource(assets)
        .insert_resource(definitions)
        .init_resource::<Assets<BehaviorDocumentAsset>>()
        .insert_resource(ZooClock {
            fractional_tick: Default::default(),
            tick: 0,
            absolute_day: 0,
            tick_in_day: 0,
        })
        .add_message::<BehaviorTaskFailed>()
        .add_message::<BehaviorTaskFinished>()
        .configure_sets(
            FixedUpdate,
            (FixedGameSet::Clock, FixedGameSet::Think).chain(),
        )
        .add_systems(
            FixedUpdate,
            begin_behavior_target_candidate_selection_updates.in_set(FixedGameSet::Clock),
        )
        .add_systems(
            FixedUpdate,
            (
                record_staff_selection_attempts,
                advance_behavior_tasks_through_return_failure_completion_and_final_outcome,
                record_guest_selection_attempts,
            )
                .chain()
                .in_set(FixedGameSet::Think),
        );
    let root = app.world_mut().spawn_empty().id();
    let target = app
        .world_mut()
        .spawn(DefinitionId(AssetId::from_key("service")))
        .id();
    let rejected = || {
        let mut delays = BehaviorTargetRetryDelays::default();
        delays.record_failure(target, policy);
        (
            WorldMember { root },
            delays,
            SelectionAttempts {
                target,
                accepted_updates: Vec::new(),
            },
        )
    };
    let guest = app.world_mut().spawn((Guest, rejected())).id();
    let staff = app.world_mut().spawn((Staff, rejected())).id();
    let document = app
        .world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .add(BehaviorDocumentAsset::from_test_document(
            BehaviorDocument::Sets(vec![]),
        ));
    let fresh_failure = app
        .world_mut()
        .spawn((
            Guest,
            WorldMember { root },
            SelectionAttempts {
                target,
                accepted_updates: Vec::new(),
            },
            PendingBehaviorTaskFailure,
            BehaviorTaskExecutionState {
                execution_id: 1,
                program: AssetId::from_key("refused-service"),
                target: Some(target),
                object: None,
                document,
                declaration: 0,
                phase: BehaviorTaskExecutionPhase::Set,
                action: 0,
                repetitions: 0,
                next_action_tick: 0,
                interaction_slot: None,
                stack: ArrayVec::new(),
            },
        ))
        .id();
    for update in 1..=51 {
        app.world_mut().resource_mut::<ZooClock>().tick = update;
        app.world_mut().run_schedule(FixedUpdate);
        if update <= 50 {
            for actor in [guest, staff] {
                assert!(app
                    .world()
                    .get::<SelectionAttempts>(actor)
                    .unwrap()
                    .accepted_updates
                    .is_empty());
            }
        }
        assert!(app
            .world()
            .get::<SelectionAttempts>(fresh_failure)
            .unwrap()
            .accepted_updates
            .is_empty());
    }
    for actor in [guest, staff] {
        assert_eq!(
            app.world()
                .get::<SelectionAttempts>(actor)
                .unwrap()
                .accepted_updates,
            [51]
        );
    }
    app.world_mut().resource_mut::<ZooClock>().tick = 52;
    app.world_mut().run_schedule(FixedUpdate);
    assert_eq!(
        app.world()
            .get::<SelectionAttempts>(fresh_failure)
            .unwrap()
            .accepted_updates,
        [52]
    );
}

#[test]
fn unsupported_nested_set_fails_instead_of_returning_success_to_its_parent() {
    let mut app = App::new();
    app.init_resource::<Assets<BehaviorDocumentAsset>>()
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .insert_resource(ZooClock {
            fractional_tick: Default::default(),
            tick: 1,
            absolute_day: 0,
            tick_in_day: 1,
        })
        .add_message::<BehaviorTaskFailed>()
        .add_message::<BehaviorTaskFinished>()
        .add_systems(
            Update,
            advance_behavior_tasks_through_return_failure_completion_and_final_outcome,
        );
    let program = AssetId::from_key("unsupported-child");
    let document = app
        .world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .add(BehaviorDocumentAsset::from_test_document(
            BehaviorDocument::Sets(vec![
                BehaviorSet {
                    id: program,
                    name: "unsupported-child".into(),
                    subjects: vec![],
                    actions: BehaviorTaskActionPhase::Unsupported {
                        first_unsupported_action: "UnmappedAction".into(),
                    },
                },
                BehaviorSet {
                    id: AssetId::from_key("parent"),
                    name: "parent".into(),
                    subjects: vec![],
                    actions: BehaviorTaskActionPhase::Supported(vec![]),
                },
            ]),
        ));
    let mut task = BehaviorTaskExecutionState {
        execution_id: 1,
        program: AssetId::from_key("parent"),
        target: None,
        document: document.clone(),
        declaration: 1,
        phase: BehaviorTaskExecutionPhase::Set,
        action: 0,
        repetitions: 0,
        next_action_tick: 0,
        object: None,
        interaction_slot: None,
        stack: ArrayVec::new(),
    };
    task.stack.push(task.create_return_frame_after_action(0));
    task.program = program;
    task.declaration = 0;
    let actor = app.world_mut().spawn(task).id();
    for tick in 1..=3 {
        app.world_mut().resource_mut::<ZooClock>().tick = tick;
        app.update();
    }
    assert!(app
        .world()
        .get::<BehaviorTaskExecutionState>(actor)
        .is_none());
    assert_eq!(
        app.world().resource::<Messages<BehaviorTaskFailed>>().len(),
        1
    );
    assert!(app
        .world()
        .resource::<Messages<BehaviorTaskFinished>>()
        .is_empty());
}
