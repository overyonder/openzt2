use bevy::prelude::*;
use openzt2_game_assets::{
    application_lifecycle::GamePhase, behavior::behavior_asset_types::BehaviorDocumentAsset,
};
use openzt2_game_data::behavior::action_record::BehaviorAction;
use openzt2_world_state::{
    application_schedule::FixedGameSet, simulation_time::simulation_clock_types::ZooClock,
};
use openzt2_zoo_state::{
    behavior_task_execution_types::{
        advance_behavior_task_to_next_action, find_current_behavior_task_action,
        BehaviorTaskExecutionState, BehaviorTaskReturnFrame, PendingBehaviorTaskFailure,
    },
    locomotion::locomotion_types::{
        Arrived, Destination, NavAgent, NavigateTo, NavigationFailed, NavigationRequestSequence,
    },
};

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;

/// An actor walking after its task target for one `BFBehPursuit` action.
#[derive(Component)]
struct PursuingForBehavior {
    execution_id: u64,
    origin: BehaviorTaskReturnFrame,
    depth: usize,
    request_id: Option<u64>,
    destination: Vec3,
}

impl PursuingForBehavior {
    fn owns(&self, task: &BehaviorTaskExecutionState) -> bool {
        self.execution_id == task.execution_id
            && self.depth == task.stack.len()
            && task.is_at_or_nested_under(&self.origin, self.depth)
    }
}

pub(super) fn register_pursuit_execution(application: &mut App) {
    application.add_systems(
        FixedUpdate,
        (finish_pursuit_navigation_requests, execute_current_pursuit_actions)
            .chain()
            .after(super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
            .before(openzt2_zoo_state::locomotion::AnimalAnimationNavigationProjection)
            .in_set(FixedGameSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    );
}

fn finish_pursuit_navigation_requests(
    mut commands: Commands,
    mut arrivals: MessageReader<Arrived>,
    mut failures: MessageReader<NavigationFailed>,
    mut pursuers: Query<&mut PursuingForBehavior>,
) {
    for failure in failures.read() {
        if pursuers
            .get(failure.entity)
            .is_ok_and(|pursuing| pursuing.request_id == Some(failure.request_id))
        {
            commands
                .entity(failure.entity)
                .remove::<PursuingForBehavior>();
            mark_behavior_task_for_failure_and_stop_navigation(failure.entity, &mut commands);
        }
    }
    // An arrival only ends this leg; the next tick measures the target again.
    for arrival in arrivals.read().filter(|arrival| arrival.target.is_none()) {
        if let Ok(mut pursuing) = pursuers.get_mut(arrival.entity) {
            if pursuing.request_id == Some(arrival.request_id) {
                pursuing.request_id = None;
            }
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn execute_current_pursuit_actions(
    mut commands: Commands,
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    mut sequence: ResMut<NavigationRequestSequence>,
    mut navigation: MessageWriter<NavigateTo>,
    mut actors: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            &GlobalTransform,
            Has<NavAgent>,
            Option<&mut PursuingForBehavior>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    transforms: Query<&GlobalTransform>,
) {
    for (actor, mut task, transform, has_agent, pursuing) in &mut actors {
        let Some(BehaviorAction::Pursuit(action)) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            if pursuing.is_some() {
                commands.entity(actor).remove::<PursuingForBehavior>();
            }
            continue;
        };
        let mut pursuing = pursuing.filter(|pursuing| pursuing.owns(&task));
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(target) = task
            .target()
            .and_then(|target| transforms.get(target).ok())
            .map(GlobalTransform::translation)
            .filter(|_| has_agent)
        else {
            commands.entity(actor).remove::<PursuingForBehavior>();
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let distance = transform.translation().xz().distance(target.xz());
        let hit_radius = action.hit_radius_cm as f32 * 0.01;
        if distance <= hit_radius {
            commands
                .entity(actor)
                .remove::<(PursuingForBehavior, Destination)>();
            advance_behavior_task_to_next_action(&mut task, clock.tick);
            continue;
        }
        if action
            .failure_radius_cm
            .is_some_and(|failure| distance > failure as f32 * 0.01)
        {
            commands.entity(actor).remove::<PursuingForBehavior>();
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        // Route again once the target has moved half the hit radius away
        // from the current destination, or the previous leg has ended.
        let needs_route = pursuing.as_ref().is_none_or(|pursuing| {
            pursuing.request_id.is_none()
                || pursuing.destination.xz().distance(target.xz()) > hit_radius * 0.5
        });
        if !needs_route {
            continue;
        }
        let request_id = sequence.next();
        navigation.write(NavigateTo {
            entity: actor,
            request_id,
            destination: target,
            arrival_radius_m: hit_radius,
        });
        match pursuing.as_deref_mut() {
            Some(pursuing) => {
                pursuing.request_id = Some(request_id);
                pursuing.destination = target;
            }
            None => {
                commands.entity(actor).insert(PursuingForBehavior {
                    execution_id: task.execution_id,
                    origin: task.create_return_frame_after_action(task.action),
                    depth: task.stack.len(),
                    request_id: Some(request_id),
                    destination: target,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::{
        behavior::{
            action::movement::BehaviorPursuitAction,
            document::{BehaviorDocument, BehaviorSet, BehaviorTaskActionPhase},
        },
        AssetId,
    };
    use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;

    #[test]
    fn pursuit_routes_to_a_far_target_finishes_in_reach_and_fails_beyond_its_radius() {
        let mut app = App::new();
        app.init_resource::<Assets<BehaviorDocumentAsset>>()
            .init_resource::<NavigationRequestSequence>()
            .insert_resource(ZooClock {
                tick: 1,
                absolute_day: 0,
                tick_in_day: 0,
            })
            .add_message::<NavigateTo>()
            .add_message::<Arrived>()
            .add_message::<NavigationFailed>()
            .add_systems(
                Update,
                (
                    finish_pursuit_navigation_requests,
                    execute_current_pursuit_actions,
                )
                    .chain(),
            );
        let program = AssetId::from_key("pursuit-test");
        let document = app
            .world_mut()
            .resource_mut::<Assets<BehaviorDocumentAsset>>()
            .add(BehaviorDocumentAsset::from_test_document(
                BehaviorDocument::Sets(vec![BehaviorSet {
                    id: program,
                    name: "pursuit-test".into(),
                    subjects: vec![],
                    actions: BehaviorTaskActionPhase::Supported(vec![BehaviorAction::Pursuit(
                        BehaviorPursuitAction {
                            locomotion_speed: None,
                            hit_radius_cm: 1_000,
                            failure_radius_cm: Some(5_000),
                        },
                    )]),
                }]),
            ));
        let target = app
            .world_mut()
            .spawn(GlobalTransform::from_translation(Vec3::new(20.0, 0.0, 0.0)))
            .id();
        let actor = app
            .world_mut()
            .spawn((
                GlobalTransform::IDENTITY,
                NavAgent {
                    radius_m: 0.3,
                    max_speed_mps: 1.0,
                    acceleration_mps2: 1.0,
                    capabilities: openzt2_zoo_state::locomotion::locomotion_types::NavFlags::GUEST,
                },
                BehaviorTaskExecutionState {
                    execution_id: 1,
                    program,
                    target: Some(target),
                    document,
                    declaration: 0,
                    phase: BehaviorTaskExecutionPhase::Set,
                    action: 0,
                    repetitions: 0,
                    next_action_tick: 0,
                    object: None,
                    interaction_slot: None,
                    stack: Default::default(),
                },
            ))
            .id();
        let routes = |app: &mut App| {
            app.world_mut()
                .resource_mut::<Messages<NavigateTo>>()
                .drain()
                .count()
        };
        app.update();
        assert_eq!(routes(&mut app), 1);
        // The target has not moved, so the leg in progress is kept.
        app.update();
        assert_eq!(routes(&mut app), 0);

        app.world_mut()
            .entity_mut(target)
            .insert(GlobalTransform::from_translation(Vec3::new(5.0, 0.0, 0.0)));
        app.update();
        assert_eq!(
            app.world()
                .get::<BehaviorTaskExecutionState>(actor)
                .unwrap()
                .action,
            1
        );

        let mut task = app
            .world_mut()
            .get_mut::<BehaviorTaskExecutionState>(actor)
            .unwrap();
        task.action = 0;
        task.next_action_tick = 0;
        app.world_mut()
            .entity_mut(target)
            .insert(GlobalTransform::from_translation(Vec3::new(60.0, 0.0, 0.0)));
        app.update();
        assert!(app
            .world()
            .get::<PendingBehaviorTaskFailure>(actor)
            .is_some());
    }
}
