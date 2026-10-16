use super::*;
use arrayvec::ArrayVec;
use openzt2_game_data::{
    behavior::{
        action::termination::BehaviorTerminationAction,
        document::{BehaviorDocument, BehaviorSet, BehaviorTaskActionPhase},
    },
    world_definitions::document::WorldDefinitionDocument,
    world_definitions::simulation_time::{
        SimulationCalendarLeapPolicy, SimulationCalendarPolicy, SimulationTimingDefinition,
    },
    AssetId,
};
use openzt2_zoo_state::animal_health::types::Frozen;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::locomotion::locomotion_types::{NavAgent, NavFlags, Route, Steering};

fn fixture(kill_subject: bool, kill_target: bool) -> (App, BehaviorTaskExecutionState) {
    let mut app = App::new();
    app.init_resource::<Assets<BehaviorDocumentAsset>>()
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .insert_resource(ZooClock {
            tick: 1,
            absolute_day: 0,
            tick_in_day: 1,
        })
        .add_message::<AnimalDied>()
        .add_systems(Update, execute_animal_termination_actions);
    let definitions = WorldDefinitionDocument {
        simulation_timing: Some(SimulationTimingDefinition {
            fixed_hz: 10,
            ticks_per_day: 10,
            speed_multipliers: vec![],
            calendar: SimulationCalendarPolicy {
                epoch_year: 2000,
                epoch_month: 1,
                epoch_day: 1,
                month_lengths: [30; 12],
                leap: SimulationCalendarLeapPolicy::None,
            },
        }),
        ..default()
    };
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(WorldDefinitionAsset::from_test_document(
            definitions.clone(),
        ));
    app.world_mut()
        .resource_mut::<WorldDefinitions>()
        .index_test_document(handle, &definitions);
    let program = AssetId::from_key("Die_Sick");
    let document = app
        .world_mut()
        .resource_mut::<Assets<BehaviorDocumentAsset>>()
        .add(BehaviorDocumentAsset::from_test_document(
            BehaviorDocument::Sets(vec![BehaviorSet {
                id: program,
                name: "Die_Sick".into(),
                subjects: vec![],
                actions: BehaviorTaskActionPhase::Supported(vec![BehaviorAction::Termination(
                    BehaviorTerminationAction {
                        kill_subject,
                        kill_target,
                        fade_out_period: std::time::Duration::from_secs(1),
                    },
                )]),
            }]),
        ));
    (
        app,
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
            next_action_tick: 0,
            interaction_slot: None,
            stack: ArrayVec::new(),
        },
    )
}

#[test]
fn authored_death_without_disease_cleans_activity_and_emits_once() {
    let (mut app, task) = fixture(true, false);
    let cause = DeathCause::Authored(task.program);
    let actor = app
        .world_mut()
        .spawn((
            Animal,
            Frozen,
            task,
            NavAgent {
                radius_m: 0.5,
                max_speed_mps: 1.0,
                acceleration_mps2: 1.0,
                capabilities: NavFlags::ANIMAL,
            },
            Route::with_capacity(1),
            Steering::default(),
        ))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<Dead>(actor),
        Some(&Dead {
            cause,
            since_tick: 1
        })
    );
    assert!(app.world().get::<Frozen>(actor).is_none());
    assert!(app.world().get::<NavAgent>(actor).is_none());
    assert!(app.world().get::<Route>(actor).is_none());
    assert!(app.world().get::<Steering>(actor).is_none());
    assert!(app
        .world()
        .get::<BehaviorTaskExecutionState>(actor)
        .is_none());
    let deaths: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<AnimalDied>>()
        .drain()
        .collect();
    assert_eq!(deaths.len(), 1);
    assert_eq!(deaths[0].animal, actor);
    assert_eq!(deaths[0].cause, cause);
    app.update();
    assert!(app.world().resource::<Messages<AnimalDied>>().is_empty());
    assert!(app.world().get::<Dead>(actor).is_some());
}

#[test]
fn kill_target_kills_the_resolved_animal_and_advances_only_on_success() {
    let (mut app, mut task) = fixture(false, true);
    let target = app.world_mut().spawn(Animal).id();
    task.target = Some(target);
    let actor = app.world_mut().spawn((Animal, task)).id();
    app.update();
    assert!(app.world().get::<Dead>(target).is_some());
    assert!(app.world().get::<Dead>(actor).is_none());
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(actor)
            .unwrap()
            .action,
        1
    );
    let deaths: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<AnimalDied>>()
        .drain()
        .collect();
    assert_eq!(deaths.len(), 1);
    assert_eq!(deaths[0].animal, target);

    let (mut app, mut task) = fixture(false, true);
    let target = app.world_mut().spawn_empty().id();
    task.target = Some(target);
    let actor = app.world_mut().spawn((Animal, task)).id();
    app.update();
    assert!(app.world().get::<Dead>(target).is_none());
    assert!(app
        .world()
        .get::<PendingBehaviorTaskFailure>(actor)
        .is_some());
    assert_eq!(
        app.world()
            .get::<BehaviorTaskExecutionState>(actor)
            .unwrap()
            .action,
        0
    );
    assert!(app.world().resource::<Messages<AnimalDied>>().is_empty());
}
