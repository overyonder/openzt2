use bevy::prelude::*;
use openzt2_game_data::{
    behavior::action_record::BehaviorAction, world_definitions::staff_management::StaffRoleKind,
};

use crate::assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::behavior_task_execution_types::advance_behavior_task_to_next_action;
use crate::plugins::behavior_task_execution_types::find_current_behavior_task_action;
use crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionState;
use crate::plugins::behavior_task_execution_types::BehaviorTaskReturnFrame;
use crate::plugins::behavior_task_execution_types::PendingBehaviorTaskFailure;
use crate::plugins::locomotion::locomotion_types::Arrived;
use crate::plugins::locomotion::locomotion_types::NavAgent;
use crate::plugins::locomotion::locomotion_types::NavigateTo;
use crate::plugins::locomotion::locomotion_types::NavigationFailed;
use crate::plugins::locomotion::locomotion_types::NavigationRequestSequence;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::staff::staff_employment_types::StaffRole;
use crate::plugins::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier;

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;

#[derive(Component, Clone, Debug)]
pub(crate) struct PendingBehaviorMoveCompletion {
    pub(crate) origin: BehaviorTaskReturnFrame,
    pub(crate) stack_depth: usize,
    pub(crate) request_id: u64,
}

pub(super) fn start_current_supported_behavior_move_actions(
    mut commands: Commands,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    simulation_clock: Res<ZooClock>,
    mut navigation_request_sequence: ResMut<NavigationRequestSequence>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut behavior_tasks: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            Option<&StaffRole>,
            Option<&NavAgent>,
        ),
        (
            Without<PendingBehaviorMoveCompletion>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
    entity_global_transforms: Query<&GlobalTransform>,
    children: Query<&Children>,
    authored_attachment_transforms: Query<(&PrefabAuthoredAttachmentIdentifier, &GlobalTransform)>,
    mut navigation_requests: MessageWriter<NavigateTo>,
) {
    for (actor, mut behavior_task, staff_role, navigation_agent) in &mut behavior_tasks {
        if behavior_task.next_action_tick > simulation_clock.tick {
            continue;
        }
        let Some(BehaviorAction::Move(move_action)) = behavior_document_assets
            .get(&behavior_task.document)
            .and_then(|document| find_current_behavior_task_action(document, &behavior_task))
        else {
            continue;
        };
        if !move_start_has_navigation_agent(navigation_agent) {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        let definitions = active_world_definitions.get(&world_definition_assets);
        if !keeper_fast_mode_uses_configured_navigation_speed(
            move_action.locomotion_speed,
            staff_role,
            navigation_agent,
            definitions,
        ) {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        let Some(target) = behavior_task.target() else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let Some(target_transform) = entity_global_transforms.get(target).ok() else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let named_node_position = move_action
            .target_node_name
            .as_deref()
            .map(|target_node_name| {
                super::behavior_docking_execution::find_named_descendant_global_transform(
                    target,
                    target_node_name,
                    &children,
                    &authored_attachment_transforms,
                )
                .map(|transform| transform.translation())
            });
        let destination = match resolve_behavior_move_destination(
            target_transform.translation(),
            named_node_position,
        ) {
            Ok(destination) => destination,
            Err(BehaviorMoveDestinationResolutionFailure::MissingTargetNode) => {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            }
        };
        let arrival_radius_m = move_action.move_radius_cm as f32 * 0.01;
        let request_id = navigation_request_sequence.next();
        navigation_requests.write(NavigateTo {
            entity: actor,
            request_id,
            destination,
            arrival_radius_m,
        });
        commands
            .entity(actor)
            .insert(PendingBehaviorMoveCompletion {
                origin: behavior_task.create_return_frame_after_action(behavior_task.action),
                stack_depth: behavior_task.stack.len(),
                request_id,
            });
        behavior_task.next_action_tick = simulation_clock.tick.saturating_add(1);
    }
}

/// Native target role: the destination is the behavior-task target entity's
/// world position; a non-empty authored `targetNode` resolves that named
/// attachment on the target instead, and a missing attachment is an explicit
/// failure rather than a walk toward an unset position.
fn resolve_behavior_move_destination(
    target_position: Vec3,
    named_node_position: Option<Option<Vec3>>,
) -> Result<Vec3, BehaviorMoveDestinationResolutionFailure> {
    match named_node_position {
        None => Ok(target_position),
        Some(Some(node_position)) => Ok(node_position),
        Some(None) => Err(BehaviorMoveDestinationResolutionFailure::MissingTargetNode),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BehaviorMoveDestinationResolutionFailure {
    MissingTargetNode,
}

/// Completes the move only for an arrival the locomotion owner reported for a
/// plain destination (no dock target) and the exact originating action/request.
/// Locomotion owns arrival tolerance; GlobalTransform can still precede its
/// latest fixed-step Transform update when this message is consumed.
pub(super) fn finish_behavior_move_actions_after_matching_arrival(
    mut commands: Commands,
    mut arrivals: MessageReader<Arrived>,
    simulation_clock: Res<ZooClock>,
    mut behavior_tasks: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            &PendingBehaviorMoveCompletion,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for arrival in arrivals.read() {
        let Ok((actor, mut behavior_task, pending_move)) = behavior_tasks.get_mut(arrival.entity)
        else {
            continue;
        };
        if arrival.target.is_some()
            || arrival.request_id != pending_move.request_id
            || !pending_move.still_owns_exact_task_frame(&behavior_task)
        {
            continue;
        }
        advance_behavior_task_to_next_action(&mut behavior_task, simulation_clock.tick);
        commands
            .entity(actor)
            .remove::<PendingBehaviorMoveCompletion>();
    }
}

pub(super) fn fail_behavior_move_actions_after_navigation_failure(
    mut commands: Commands,
    mut failures: MessageReader<NavigationFailed>,
    waiting_tasks: Query<(&BehaviorTaskExecutionState, &PendingBehaviorMoveCompletion)>,
) {
    for failure in failures.read() {
        let waiting_for_move =
            waiting_tasks
                .get(failure.entity)
                .ok()
                .is_some_and(|(task, pending)| {
                    failure.request_id == pending.request_id
                        && pending.still_owns_exact_task_frame(task)
                });
        if waiting_for_move {
            mark_behavior_task_for_failure_and_stop_navigation(failure.entity, &mut commands);
        }
    }
}

fn move_start_has_navigation_agent(navigation_agent: Option<&NavAgent>) -> bool {
    navigation_agent.is_some()
}

fn keeper_fast_mode_uses_configured_navigation_speed(
    locomotion_speed: Option<
        openzt2_game_data::behavior::action::movement::BehaviorMoveLocomotionSpeed,
    >,
    staff_role: Option<&StaffRole>,
    navigation_agent: Option<&NavAgent>,
    definitions: Option<WorldDefinitionsView<'_>>,
) -> bool {
    match locomotion_speed {
        None => true,
        Some(openzt2_game_data::behavior::action::movement::BehaviorMoveLocomotionSpeed::Fast) => {
            let (Some(staff_role), Some(navigation_agent), Some(definitions)) =
                (staff_role, navigation_agent, definitions)
            else {
                return false;
            };
            let Some(staff_definition) = definitions.find_staff(staff_role.0) else {
                return false;
            };
            staff_definition.role == StaffRoleKind::Keeper
                && navigation_agent.max_speed_mps == staff_definition.move_speed_mps
        }
        Some(_) => false,
    }
}

impl PendingBehaviorMoveCompletion {
    fn still_owns_exact_task_frame(&self, task: &BehaviorTaskExecutionState) -> bool {
        task.stack.len() == self.stack_depth
            && task.is_at_or_nested_under(&self.origin, self.stack_depth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::locomotion::locomotion_types::NavFlags;

    fn test_move_task() -> BehaviorTaskExecutionState {
        BehaviorTaskExecutionState {
            execution_id: 1,
            program: openzt2_game_data::AssetId::from_key("test:move"),
            target: None,
            document: Handle::default(),
            declaration: 0,
            phase:
                crate::plugins::behavior_task_execution_types::BehaviorTaskExecutionPhase::Execution,
            action: 0,
            repetitions: 0,
            next_action_tick: 1,
            interaction_slot: None,
            stack: Default::default(),
        }
    }

    #[test]
    fn same_tick_arrival_completes_and_duplicate_arrival_is_ignored() {
        let mut app = App::new();
        app.insert_resource(ZooClock {
            tick: 0,
            absolute_day: 0,
            tick_in_day: 0,
        })
        .add_message::<Arrived>()
        .add_systems(Update, finish_behavior_move_actions_after_matching_arrival);
        let task = test_move_task();
        let origin = task.create_return_frame_after_action(task.action);
        let actor = app
            .world_mut()
            .spawn((
                task,
                PendingBehaviorMoveCompletion {
                    origin,
                    stack_depth: 0,
                    request_id: 1,
                },
                NavAgent {
                    radius_m: 0.5,
                    max_speed_mps: 1.0,
                    acceleration_mps2: 1.0,
                    capabilities: NavFlags::STAFF,
                },
                GlobalTransform::IDENTITY,
            ))
            .id();
        app.world_mut().write_message(Arrived {
            entity: actor,
            request_id: 1,
            target: None,
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<BehaviorTaskExecutionState>(actor)
                .unwrap()
                .action,
            1
        );
        assert!(app
            .world()
            .get::<PendingBehaviorMoveCompletion>(actor)
            .is_none());
        app.world_mut().write_message(Arrived {
            entity: actor,
            request_id: 1,
            target: None,
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<BehaviorTaskExecutionState>(actor)
                .unwrap()
                .action,
            1
        );
    }

    #[test]
    fn nested_set_move_advances_the_current_nested_action() {
        let mut app = App::new();
        app.insert_resource(ZooClock {
            tick: 0,
            absolute_day: 0,
            tick_in_day: 0,
        })
        .add_message::<Arrived>()
        .add_systems(Update, finish_behavior_move_actions_after_matching_arrival);
        let mut task = test_move_task();
        let parent = task.create_return_frame_after_action(7);
        assert!(task.push_return_frame(parent));
        let origin = task.create_return_frame_after_action(task.action);
        let actor = app
            .world_mut()
            .spawn((
                task,
                PendingBehaviorMoveCompletion {
                    origin,
                    stack_depth: 1,
                    request_id: 1,
                },
                NavAgent {
                    radius_m: 0.5,
                    max_speed_mps: 1.0,
                    acceleration_mps2: 1.0,
                    capabilities: NavFlags::STAFF,
                },
                GlobalTransform::IDENTITY,
            ))
            .id();
        app.world_mut().write_message(Arrived {
            entity: actor,
            request_id: 1,
            target: None,
        });
        app.update();
        let task = app
            .world()
            .get::<BehaviorTaskExecutionState>(actor)
            .unwrap();
        assert_eq!(task.action, 1);
        assert_eq!(task.stack.len(), 1);
    }

    #[test]
    fn superseded_move_arrival_is_ignored_and_duplicate_failure_is_consumed_once() {
        let mut app = App::new();
        app.insert_resource(ZooClock {
            tick: 1,
            absolute_day: 0,
            tick_in_day: 1,
        })
        .add_message::<Arrived>()
        .add_systems(Update, finish_behavior_move_actions_after_matching_arrival);
        let task = test_move_task();
        let origin = task.create_return_frame_after_action(task.action);
        let actor = app
            .world_mut()
            .spawn((
                task,
                PendingBehaviorMoveCompletion {
                    origin,
                    stack_depth: 0,
                    request_id: 1,
                },
                NavAgent {
                    radius_m: 0.5,
                    max_speed_mps: 1.0,
                    acceleration_mps2: 1.0,
                    capabilities: NavFlags::STAFF,
                },
                GlobalTransform::IDENTITY,
            ))
            .id();
        app.world_mut()
            .get_mut::<BehaviorTaskExecutionState>(actor)
            .unwrap()
            .program = openzt2_game_data::AssetId::from_key("test:superseding");
        app.world_mut().write_message(Arrived {
            entity: actor,
            request_id: 2,
            target: None,
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<BehaviorTaskExecutionState>(actor)
                .unwrap()
                .action,
            0
        );
        assert!(app
            .world()
            .get::<PendingBehaviorMoveCompletion>(actor)
            .is_some());

        let mut failure_app = App::new();
        failure_app
            .add_message::<NavigationFailed>()
            .add_systems(Update, fail_behavior_move_actions_after_navigation_failure);
        let failure_task = test_move_task();
        let failure_origin = failure_task.create_return_frame_after_action(failure_task.action);
        let failure_actor = failure_app
            .world_mut()
            .spawn((
                failure_task,
                PendingBehaviorMoveCompletion {
                    origin: failure_origin,
                    stack_depth: 0,
                    request_id: 1,
                },
            ))
            .id();
        failure_app.world_mut().write_message(NavigationFailed {
            entity: failure_actor,
            request_id: 1,
            reason: crate::plugins::locomotion::locomotion_types::NavigationFailure::NoRoute,
        });
        failure_app.update();
        assert!(failure_app
            .world()
            .get::<PendingBehaviorMoveCompletion>(failure_actor)
            .is_none());
        assert!(failure_app
            .world()
            .get::<crate::plugins::behavior_task_execution_types::PendingBehaviorTaskFailure>(
                failure_actor
            )
            .is_some());
        failure_app.world_mut().write_message(NavigationFailed {
            entity: failure_actor,
            request_id: 1,
            reason: crate::plugins::locomotion::locomotion_types::NavigationFailure::NoRoute,
        });
        failure_app.update();
    }

    #[test]
    fn same_request_failure_wins_over_same_frame_arrival() {
        let mut app = App::new();
        app.insert_resource(ZooClock {
            tick: 1,
            absolute_day: 0,
            tick_in_day: 1,
        })
        .add_message::<Arrived>()
        .add_message::<NavigationFailed>()
        .add_systems(
            Update,
            (
                fail_behavior_move_actions_after_navigation_failure,
                finish_behavior_move_actions_after_matching_arrival,
            )
                .chain(),
        );
        let task = test_move_task();
        let origin = task.create_return_frame_after_action(task.action);
        let actor = app
            .world_mut()
            .spawn((
                task,
                PendingBehaviorMoveCompletion {
                    origin,
                    stack_depth: 0,
                    request_id: 1,
                },
                NavAgent {
                    radius_m: 0.5,
                    max_speed_mps: 1.0,
                    acceleration_mps2: 1.0,
                    capabilities: NavFlags::STAFF,
                },
                GlobalTransform::IDENTITY,
            ))
            .id();
        app.world_mut().write_message(NavigationFailed {
            entity: actor,
            request_id: 1,
            reason: crate::plugins::locomotion::locomotion_types::NavigationFailure::NoRoute,
        });
        app.world_mut().write_message(Arrived {
            entity: actor,
            request_id: 1,
            target: None,
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<BehaviorTaskExecutionState>(actor)
                .unwrap()
                .action,
            0
        );
        assert!(app
            .world()
            .get::<PendingBehaviorTaskFailure>(actor)
            .is_some());
        assert!(app
            .world()
            .get::<PendingBehaviorMoveCompletion>(actor)
            .is_none());
    }
}
