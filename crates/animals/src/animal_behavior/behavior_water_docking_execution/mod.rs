use super::{
    behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
    natural_water_targets::NaturalWaterTargets,
};
use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::{
    animation_graph::animation_graph_playback_message_types::{
        AnimationClipPlaybackRequest, AnimationClipPlaybackRequestRejected,
        AnimationClipPlaybackRequestStarted, CancelAnimationClipPlaybackRequest,
    },
    animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner,
    animation_playback::{
        animation_event_message_types::AnimationCompleted,
        animation_playback_controller_types::{
            AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
        },
    },
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_entity_crating::{AnimalCrate, ScriptedCrating},
};
use openzt2_zoo_state::animal_behavior::behavior_natural_water_target_types::SelectedNaturalWaterTarget;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    BehaviorInstructionId, RunBehaviorDockWater, RunningBehaviorInstruction,
};
use openzt2_zoo_state::{
    animal_health::types::{Dead, Escaped, Frozen, Tranquilized},
    behavior_task_execution_types::{
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure, SamplingBehaviorDockPose,
    },
    locomotion::locomotion_types::{
        Arrived, Destination, DockAt, Docking, NavigationFailed, NavigationRequestGeneration,
    },
};

#[derive(Clone, Copy)]
enum WaterDockStage {
    Sampling {
        request: usize,
        playback: Option<(Entity, u64)>,
    },
    Ready {
        mouth: Vec3,
    },
    Moving,
    Arrived,
    Finishing {
        request: usize,
        playback: Option<(Entity, u64)>,
    },
    Finished,
    Failed,
}

#[derive(Component)]
struct WaterDock {
    instruction: BehaviorInstructionId,
    shore: SelectedNaturalWaterTarget,
    node: String,
    clip_request: Option<usize>,
    stage: WaterDockStage,
}

#[derive(Component)]
struct SampleStartedWaterDockPose;

#[derive(Component)]
struct MovingWaterDock {
    request: u64,
    issued_at: u64,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ExecuteWaterDock;

impl WaterDock {
    fn owns(&self, task: &BehaviorTaskExecutionState, step: &RunningBehaviorInstruction) -> bool {
        self.instruction == step.id()
            && task.target() == Some(self.shore.region)
            && step.is_current(task)
    }
}

pub(super) fn register(application: &mut App) {
    use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
        activated_behavior_actor, update_waiting_behavior_actors,
    };
    application.add_observer(
        activated_behavior_actor::<
            openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorDockWater,
        >
            .pipe(execute_water_docking),
    );

    use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorStepFixedSet;
    application
        .add_systems(
            FixedUpdate,
            (
                receive_water_docking_events,
                update_waiting_behavior_actors
                    .pipe(execute_water_docking)
                    .in_set(ExecuteWaterDock)
                    .after(receive_water_docking_events),
                revalidate_moving_water_docks.after(ExecuteWaterDock),
            )
                .in_set(BehaviorStepFixedSet::Think)
                .run_if(in_state(GamePhase::InGame)),
        )
        .add_systems(
            PostUpdate,
            sample_started_docking_poses
                .after(super::behavior_named_nodes::ProjectBehaviorNamedNodes)
                .run_if(in_state(GamePhase::InGame)),
        );
}

fn receive_water_docking_events(
    mut commands: Commands,
    mut starts: MessageReader<AnimationClipPlaybackRequestStarted>,
    mut rejections: MessageReader<AnimationClipPlaybackRequestRejected>,
    mut completed: MessageReader<AnimationCompleted>,
    mut arrivals: MessageReader<Arrived>,
    mut failures: MessageReader<NavigationFailed>,
    owners: Query<&AnimationPresentationOwner, With<AnimationPlaybackController>>,
    mut docks: Query<(Entity, &mut WaterDock, Option<&MovingWaterDock>)>,
) {
    for event in starts.read() {
        let Ok((_, mut dock, _)) = docks.get_mut(event.animation_subject_entity) else {
            continue;
        };
        match &mut dock.stage {
            WaterDockStage::Sampling { request, playback } if *request == event.request_id => {
                *playback = Some((event.controller_entity, event.playback_generation));
                commands
                    .entity(event.animation_subject_entity)
                    .insert(SampleStartedWaterDockPose);
            }
            WaterDockStage::Finishing { request, playback } if *request == event.request_id => {
                *playback = Some((event.controller_entity, event.playback_generation));
            }
            _ => {}
        }
    }
    for event in rejections.read() {
        let Ok((_, mut dock, _)) = docks.get_mut(event.animation_subject_entity) else {
            continue;
        };
        if matches!(dock.stage, WaterDockStage::Sampling { request, .. } | WaterDockStage::Finishing { request, .. } if request == event.request_id)
        {
            dock.stage = WaterDockStage::Failed;
            commands
                .entity(event.animation_subject_entity)
                .remove::<SampleStartedWaterDockPose>();
        }
    }
    for event in completed.read() {
        let controller = event.animation_playback_controller_entity;
        let actor = owners
            .get(controller)
            .map_or(controller, |owner| owner.gameplay_entity);
        let Ok((_, mut dock, _)) = docks.get_mut(actor) else {
            continue;
        };
        let WaterDockStage::Finishing {
            request,
            playback: Some((expected_controller, generation)),
        } = dock.stage
        else {
            continue;
        };
        if event.explicit_clip_request_id == Some(request)
            && controller == expected_controller
            && event.playback_generation == generation
        {
            dock.stage = WaterDockStage::Finished;
        }
    }
    for event in arrivals.read() {
        if let Ok((_, mut dock, moving)) = docks.get_mut(event.entity) {
            if moving.is_some_and(|moving| moving.request == event.request_id)
                && matches!(dock.stage, WaterDockStage::Moving)
                && event.target == Some(dock.shore.region)
            {
                dock.stage = WaterDockStage::Arrived;
                commands.entity(event.entity).remove::<MovingWaterDock>();
            }
        }
    }
    for event in failures.read() {
        if let Ok((_, mut dock, moving)) = docks.get_mut(event.entity) {
            if moving.is_some_and(|moving| moving.request == event.request_id)
                && matches!(dock.stage, WaterDockStage::Moving)
            {
                dock.stage = WaterDockStage::Failed;
                commands.entity(event.entity).remove::<MovingWaterDock>();
            }
        }
    }
}

fn execute_water_docking(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    clocks: Query<&ZooClock>,
    shores: NaturalWaterTargets,
    transforms: Query<&GlobalTransform>,
    controllers: Query<&AnimationPlaybackController>,
    movement: Query<Ref<Destination>>,
    escaped: Query<Ref<Escaped>>,
    mut navigation_generations: Query<&mut NavigationRequestGeneration>,
    mut navigation: MessageWriter<DockAt>,
    mut clips: MessageWriter<AnimationClipPlaybackRequest>,
    mut cancellations: MessageWriter<CancelAnimationClipPlaybackRequest>,
    mut actors: Query<
        (
            Entity,
            &WorldMember,
            Option<&BehaviorTaskExecutionState>,
            Option<(&RunningBehaviorInstruction, &RunBehaviorDockWater)>,
            Option<&SelectedNaturalWaterTarget>,
            Option<&mut WaterDock>,
            Option<&MovingWaterDock>,
            Has<PendingBehaviorTaskFailure>,
            (Has<AnimalCrate>, Has<ScriptedCrating>),
            Has<Dead>,
            Has<Frozen>,
            Has<Tranquilized>,
        ),
        Or<(With<RunBehaviorDockWater>, With<WaterDock>)>,
    >,
) {
    for (
        actor,
        member,
        task,
        running,
        selected,
        pending,
        moving,
        failing,
        crated,
        dead,
        frozen,
        tranquilized,
    ) in behavior_action_actors(&mut actors, activation.0)
    {
        let Ok(clock) = clocks.get(member.root) else {
            continue;
        };
        let mut pending = pending;
        let invalid = failing || crated.0 || crated.1 || dead || frozen || tranquilized;
        if let Some(dock) = pending.as_deref() {
            if invalid
                || task
                    .zip(running)
                    .is_none_or(|(task, (step, _))| !dock.owns(task, step))
                || (shores.water_inputs_changed(member.root, &dock.shore)
                    && !shores.valid(member.root, &dock.shore))
            {
                if let Some(request) = dock.clip_request {
                    cancellations.write(CancelAnimationClipPlaybackRequest {
                        animation_subject_entity: actor,
                        request_id: request,
                    });
                }
                if moving.is_some_and(|moving| {
                    movement
                        .get(actor)
                        .is_ok_and(|destination| destination.request_id == moving.request)
                }) {
                    commands.entity(actor).remove::<(Destination, Docking)>();
                }
                if !invalid
                    && task
                        .zip(running)
                        .is_some_and(|(task, (step, _))| dock.owns(task, step))
                {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                }
                commands.entity(actor).remove::<(
                    WaterDock,
                    SamplingBehaviorDockPose,
                    SampleStartedWaterDockPose,
                    MovingWaterDock,
                    SelectedNaturalWaterTarget,
                )>();
                continue;
            }
        }
        let Some(task) = task.filter(|task| !invalid && task.next_action_tick <= clock.tick) else {
            continue;
        };
        let Some((instruction, running)) =
            running.filter(|(instruction, _)| instruction.is_current(task))
        else {
            continue;
        };
        if activation.0.is_none() && !instruction.can_update_after_activation(clock.tick) {
            continue;
        }
        let action = &running.0;
        let Some(shore) = pending.as_deref().map(|dock| dock.shore).or_else(|| {
            selected.copied().filter(|shore| {
                task.target() == Some(shore.region) && shores.valid(member.root, shore)
            })
        }) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        if let Some(dock) = pending.as_deref_mut() {
            match dock.stage {
                WaterDockStage::Ready { mouth } => {
                    let (Ok(actor_transform), Ok(target_transform), Ok(agent)) = (
                        transforms.get(actor),
                        transforms.get(shore.region),
                        shores.agents.get(actor),
                    ) else {
                        continue;
                    };
                    let direction = (shore.water - shore.approach).xz().normalize_or_zero();
                    let rotation = Quat::from_rotation_y(direction.x.atan2(direction.y));
                    let (scale, _, _) = actor_transform.to_scale_rotation_translation();
                    let point = shore.water - rotation * (scale * mouth);
                    let escaped = escaped.contains(actor);
                    if !point.is_finite()
                        || !shores.docking_endpoint(member.root, actor, point, &shore, escaped)
                    {
                        dock.stage = WaterDockStage::Failed;
                        continue;
                    }
                    let Ok(mut generation) = navigation_generations.get_mut(actor) else {
                        continue;
                    };
                    let request = generation.advance();
                    navigation.write(DockAt {
                        entity: actor,
                        request_id: request,
                        target: shore.region,
                        local_point: target_transform.affine().inverse().transform_point3(point),
                        local_forward: target_transform
                            .affine()
                            .inverse()
                            .transform_vector3(Vec3::new(direction.x, 0.0, direction.y)),
                        radius_m: agent.radius_m.max(0.05),
                    });
                    dock.stage = WaterDockStage::Moving;
                    commands.entity(actor).insert(MovingWaterDock {
                        request,
                        issued_at: clock.tick,
                    });
                }
                WaterDockStage::Arrived => {
                    let request = clips
                        .write(AnimationClipPlaybackRequest {
                            animation_subject_entity: actor,
                            animation_clip_asset_key: action
                                .target_animation_clip_asset_key
                                .clone(),
                            blend_duration_milliseconds: 0,
                            playback_speed_permille: 1000,
                            playback_repetition_policy: AnimationPlaybackRepetitionPolicy::PlayOnce,
                            action_settings: None,
                        })
                        .id;
                    dock.stage = WaterDockStage::Finishing {
                        request,
                        playback: None,
                    };
                    dock.clip_request = Some(request);
                    commands.entity(actor).insert(SamplingBehaviorDockPose);
                }
                WaterDockStage::Finished => {
                    if let Some(request) = dock.clip_request.take() {
                        cancellations.write(CancelAnimationClipPlaybackRequest {
                            animation_subject_entity: actor,
                            request_id: request,
                        });
                    }
                    commands
                        .entity(actor)
                        .remove::<(WaterDock, SamplingBehaviorDockPose)>();
                    instruction.complete(actor, clock.tick, &mut commands);
                }
                WaterDockStage::Failed => {
                    if let Some(request) = dock.clip_request.take() {
                        cancellations.write(CancelAnimationClipPlaybackRequest {
                            animation_subject_entity: actor,
                            request_id: request,
                        });
                    }
                    commands
                        .entity(actor)
                        .remove::<(WaterDock, SamplingBehaviorDockPose)>();
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                }
                WaterDockStage::Finishing {
                    request,
                    playback: Some((controller, generation)),
                } => {
                    if !controllers.get(controller).is_ok_and(|controller| {
                        controller.explicit_clip_request_id == Some(request)
                            && controller.playback_generation == generation
                    }) {
                        dock.stage = WaterDockStage::Failed;
                    }
                }
                _ => {}
            }
        } else {
            let request = clips
                .write(AnimationClipPlaybackRequest {
                    animation_subject_entity: actor,
                    animation_clip_asset_key: action.target_animation_clip_asset_key.clone(),
                    blend_duration_milliseconds: 0,
                    playback_speed_permille: 1000,
                    playback_repetition_policy: AnimationPlaybackRepetitionPolicy::Loop,
                    action_settings: None,
                })
                .id;
            commands.entity(actor).insert((
                SamplingBehaviorDockPose,
                WaterDock {
                    instruction: instruction.id(),
                    shore,
                    node: action.subject_node_name.clone(),
                    clip_request: Some(request),
                    stage: WaterDockStage::Sampling {
                        request,
                        playback: None,
                    },
                },
            ));
        }
    }
}

fn revalidate_moving_water_docks(
    mut commands: Commands,
    shores: NaturalWaterTargets,
    clocks: Query<&ZooClock>,
    movement: Query<Ref<Destination>>,
    escapes: Query<Ref<Escaped>>,
    mut removed_escapes: RemovedComponents<Escaped>,
    mut docks: Query<(Entity, &WorldMember, &MovingWaterDock, &mut WaterDock)>,
) {
    let removed_escapes = removed_escapes.read().collect::<EntityHashSet>();
    for (actor, member, moving, mut dock) in &mut docks {
        if clocks
            .get(member.root)
            .is_ok_and(|clock| clock.tick <= moving.issued_at)
        {
            continue;
        }
        let valid = movement.get(actor).is_ok_and(|destination| {
            let escape = escapes.get(actor).ok();
            let point = destination.world;
            let escaped = escape.is_some();
            let revalidate = destination.is_changed()
                || escape.as_ref().is_some_and(DetectChanges::is_changed)
                || removed_escapes.contains(&actor)
                || shores.docking_inputs_changed(member.root, actor, point, &dock.shore, escaped);
            destination.request_id == moving.request
                && (!revalidate
                    || shores.docking_endpoint(member.root, actor, point, &dock.shore, escaped))
        });
        if !valid {
            dock.stage = WaterDockStage::Failed;
            commands.entity(actor).remove::<MovingWaterDock>();
        }
    }
}

fn sample_started_docking_poses(
    mut commands: Commands,
    mut docks: Query<(Entity, &mut WaterDock), With<SampleStartedWaterDockPose>>,
    controllers: Query<&AnimationPlaybackController>,
    transforms: Query<&GlobalTransform>,
    nodes: super::behavior_named_nodes::NamedNodeTransforms,
    mut cancellations: MessageWriter<CancelAnimationClipPlaybackRequest>,
) {
    for (actor, mut dock) in &mut docks {
        commands
            .entity(actor)
            .remove::<SampleStartedWaterDockPose>();
        let WaterDockStage::Sampling {
            request,
            playback: Some((controller, generation)),
        } = dock.stage
        else {
            continue;
        };
        let Ok(playback) = controllers.get(controller) else {
            dock.stage = WaterDockStage::Failed;
            continue;
        };
        if playback.explicit_clip_request_id != Some(request)
            || playback.playback_generation != generation
        {
            dock.stage = WaterDockStage::Failed;
            continue;
        }
        let node = nodes.pose(actor, &dock.node);
        let mouth = node.zip(transforms.get(actor).ok()).map(|(node, actor)| {
            actor
                .affine()
                .inverse()
                .transform_point3(node.translation())
        });
        cancellations.write(CancelAnimationClipPlaybackRequest {
            animation_subject_entity: actor,
            request_id: request,
        });
        dock.clip_request = None;
        commands.entity(actor).remove::<SamplingBehaviorDockPose>();
        dock.stage = mouth
            .filter(|mouth| mouth.is_finite())
            .map_or(WaterDockStage::Failed, |mouth| WaterDockStage::Ready {
                mouth,
            });
    }
}
