use super::*;
use openzt2_game_data::{
    behavior::document::{BehaviorDocument, BehaviorSet, BehaviorTaskActionPhase},
    AssetId,
};
use openzt2_zoo_state::animal_behavior::behavior_target_retry_delays::{
    restore_behavior_target_retry, BehaviorTargetRejections,
};

#[derive(Component)]
struct SelectionAttempts {
    target: Entity,
    accepted_updates: Vec<u64>,
}

fn record_guest_selection_attempts(
    clock: Res<ZooClock>,
    rejections: BehaviorTargetRejections,
    mut actors: Query<
        (Entity, &mut SelectionAttempts),
        (
            With<openzt2_zoo_state::guests::guest_simulation_types::Guest>,
            Without<BehaviorTaskExecutionState>,
        ),
    >,
) {
    for (actor, mut attempts) in &mut actors {
        if !rejections.rejects(actor, attempts.target) {
            attempts.accepted_updates.push(clock.tick);
        }
    }
}

fn record_staff_selection_attempts(
    clock: Res<ZooClock>,
    rejections: BehaviorTargetRejections,
    mut actors: Query<
        (Entity, &mut SelectionAttempts),
        (
            With<openzt2_zoo_state::staff::staff_employment_types::Staff>,
            Without<BehaviorTaskExecutionState>,
        ),
    >,
) {
    for (actor, mut attempts) in &mut actors {
        if !rejections.rejects(actor, attempts.target) {
            attempts.accepted_updates.push(clock.tick);
        }
    }
}

#[test]
fn guest_and_staff_selection_share_the_window_on_opposite_sides_of_behavior_updates() {
    for (interval, cap) in [(50, 2000), (2, 5)] {
        check_guest_and_staff_selection_window(interval, cap);
    }
}

fn check_guest_and_staff_selection_window(interval: u32, cap: u32) {
    use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;
    use openzt2_world_state::application_schedule::FixedGameSet;
    use openzt2_zoo_state::guests::guest_simulation_types::Guest;
    use openzt2_zoo_state::staff::staff_employment_types::Staff;

    let policy = BehaviorSelectionPolicy {
        enabled: true,
        minimum_score: 0.0,
        outside_range_need_value: 0.0,
        rejected_target_retry_interval: interval,
        rejected_target_retry_interval_cap: cap,
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
                advance_behavior_target_retry_clocks,
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
        (
            WorldMember { root },
            SelectionAttempts {
                target,
                accepted_updates: Vec::new(),
            },
        )
    };
    let guest = app.world_mut().spawn((Guest, rejected())).id();
    let staff = app.world_mut().spawn((Staff, rejected())).id();
    for actor in [guest, staff] {
        restore_behavior_target_retry(app.world_mut(), actor, target, 1, interval.min(cap));
    }
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
                document: document.clone(),
                declaration: 0,
                phase: BehaviorTaskExecutionPhase::Set,
                action: 0,
                repetitions: 0,
                next_action_tick: 0,
                interaction_slot: None,
                depth: 0,
            },
        ))
        .id();
    let first_available_update = u64::from(interval) + 1;
    for update in 1..=first_available_update {
        app.world_mut().resource_mut::<ZooClock>().tick = update;
        app.world_mut().run_schedule(FixedUpdate);
        if update < first_available_update {
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
            [first_available_update]
        );
    }
    app.world_mut().resource_mut::<ZooClock>().tick = first_available_update + 1;
    app.world_mut().run_schedule(FixedUpdate);
    assert_eq!(
        app.world()
            .get::<SelectionAttempts>(fresh_failure)
            .unwrap()
            .accepted_updates,
        [first_available_update + 1]
    );

    if interval != 2 || cap != 5 {
        return;
    }
    let mut update = first_available_update + 1;
    for expected_delay in [4, 5, 5] {
        for actor in [guest, staff] {
            app.world_mut()
                .get_mut::<SelectionAttempts>(actor)
                .unwrap()
                .accepted_updates
                .clear();
            app.world_mut().entity_mut(actor).insert((
                PendingBehaviorTaskFailure,
                BehaviorTaskExecutionState {
                    execution_id: update,
                    program: AssetId::from_key("refused-service"),
                    target: Some(target),
                    object: None,
                    document: document.clone(),
                    declaration: 0,
                    phase: BehaviorTaskExecutionPhase::Set,
                    action: 0,
                    repetitions: 0,
                    next_action_tick: 0,
                    interaction_slot: None,
                    depth: 0,
                },
            ));
        }
        let failure_update = update + 1;
        let available_update = failure_update + expected_delay + 1;
        while update < available_update {
            update += 1;
            app.world_mut().resource_mut::<ZooClock>().tick = update;
            app.world_mut().run_schedule(FixedUpdate);
            for actor in [guest, staff] {
                let attempts = &app
                    .world()
                    .get::<SelectionAttempts>(actor)
                    .unwrap()
                    .accepted_updates;
                if update < available_update {
                    assert!(attempts.is_empty(), "actor {actor:?} retried at {update}");
                } else {
                    assert_eq!(attempts, &[available_update]);
                }
            }
        }
    }
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
        depth: 0,
    };
    let return_frame = task.create_return_frame_after_action(0);
    task.depth = 1;
    task.program = program;
    task.declaration = 0;
    let actor = app.world_mut().spawn(task).id();
    app.world_mut().spawn((
        openzt2_zoo_state::behavior_task_execution_types::continuations::BehaviorContinuationOf(
            actor,
        ),
        openzt2_zoo_state::behavior_task_execution_types::continuations::SuspendedBehaviorFrame(
            return_frame,
        ),
    ));
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

#[test]
fn consecutive_instructions_finish_in_authored_order_and_a_wait_runs_once() {
    use openzt2_game_data::behavior::{
        action::head_look::BehaviorHeadLookAction, action_record::BehaviorAction,
    };
    use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
        apply_behavior_instruction_completion, defer_incomplete_behavior_steps,
        drive_behavior_steps, BehaviorStep, BehaviorStepProgress, BehaviorStepSet,
        RunBehaviorHeadLook, RunBehaviorSendSubjectTrigger,
    };

    #[derive(Component, Default)]
    struct Trace(Vec<usize>);

    fn head_look(
        clock: Res<ZooClock>,
        mut actors: Query<
            (
                Entity,
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &mut Trace,
            ),
            With<RunBehaviorHeadLook>,
        >,
        mut commands: Commands,
    ) {
        for (actor, task, instruction, mut trace) in &mut actors {
            if task.next_action_tick <= clock.tick {
                trace.0.push(task.action);
                instruction.complete(actor, clock.tick, &mut commands);
            }
        }
    }

    fn subject_trigger(
        clock: Res<ZooClock>,
        mut actors: Query<
            (
                Entity,
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &mut Trace,
            ),
            With<RunBehaviorSendSubjectTrigger>,
        >,
        mut commands: Commands,
    ) {
        for (actor, task, instruction, mut trace) in &mut actors {
            if task.next_action_tick <= clock.tick {
                trace.0.push(task.action);
                if task.action == 1 {
                    instruction.complete(actor, clock.tick, &mut commands);
                }
            }
        }
    }

    let mut app = App::new();
    app.init_resource::<Assets<BehaviorDocumentAsset>>()
        .init_resource::<BehaviorStepProgress>()
        .insert_resource(ZooClock {
            tick: 7,
            fractional_tick: Default::default(),
            absolute_day: 0,
            tick_in_day: 7,
        })
        .init_schedule(BehaviorStep)
        .configure_sets(
            BehaviorStep,
            (BehaviorStepSet::Actions, BehaviorStepSet::Finish).chain(),
        )
        .add_observer(apply_behavior_instruction_completion)
        .add_systems(
            BehaviorStep,
            (subject_trigger, head_look).in_set(BehaviorStepSet::Actions),
        )
        .add_systems(
            BehaviorStep,
            defer_incomplete_behavior_steps.in_set(BehaviorStepSet::Finish),
        );
    let look = BehaviorAction::HeadLook(BehaviorHeadLookAction {
        disengage: false,
        target_node: None,
    });
    let trigger = BehaviorAction::SendSubjectTrigger(AssetId::from_key("trigger"));
    let program = AssetId::from_key("sequence");
    let document = app
        .world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .add(BehaviorDocumentAsset::from_test_document(
            BehaviorDocument::Sets(vec![BehaviorSet {
                id: program,
                name: "sequence".to_owned(),
                subjects: Vec::new(),
                actions: BehaviorTaskActionPhase::Supported(vec![
                    look.clone(),
                    trigger.clone(),
                    look,
                    trigger,
                ]),
            }]),
        ));
    let actor = app
        .world_mut()
        .spawn((
            Trace::default(),
            BehaviorTaskExecutionState {
                execution_id: 1,
                program,
                target: None,
                object: None,
                document,
                declaration: 0,
                phase: BehaviorTaskExecutionPhase::Set,
                action: 0,
                repetitions: 0,
                next_action_tick: 7,
                interaction_slot: None,
                depth: 0,
            },
        ))
        .id();
    app.world_mut().flush();

    drive_behavior_steps(app.world_mut());
    drive_behavior_steps(app.world_mut());
    assert_eq!(app.world().get::<Trace>(actor).unwrap().0, [0, 1, 2, 3]);
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(actor)
            .unwrap()
            .next_action_tick,
        8
    );
}
