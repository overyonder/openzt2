use super::{
    behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
    natural_water_targets::NaturalWaterTargets,
};
use bevy::prelude::*;
use openzt2_game_assets::{
    application_lifecycle::GamePhase, behavior::behavior_asset_types::BehaviorDocumentAsset,
};
use openzt2_game_data::behavior::action_record::BehaviorAction;
use openzt2_world_state::{
    animation_graph::animation_graph_playback_message_types::{
        AnimationClipPlaybackRequest, AnimationClipPlaybackRequestRejected,
        AnimationClipPlaybackRequestStarted, CancelAnimationClipPlaybackRequest,
    },
    animation_playback::{
        animation_event_message_types::AnimationCompleted,
        animation_playback_controller_types::{
            AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
        },
    },
    application_schedule::FixedGameSet,
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_entity_crating::WorldEntityIsCrated,
};
use openzt2_zoo_state::animal_behavior::behavior_natural_water_target_types::SelectedNaturalWaterTarget;
use openzt2_zoo_state::{
    animal_health::types::{Dead, Frozen, Tranquilized},
    behavior_task_execution_types::{
        advance_behavior_task_to_next_action, find_current_behavior_task_action,
        BehaviorTaskExecutionState, BehaviorTaskReturnFrame, PendingBehaviorTaskFailure,
        SamplingBehaviorDockPose,
    },
    locomotion::locomotion_types::{
        Arrived, Destination, DockAt, Docking, NavigationFailed, NavigationRequestSequence,
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
    Moving {
        request: u64,
    },
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
    execution: u64,
    origin: BehaviorTaskReturnFrame,
    depth: usize,
    shore: SelectedNaturalWaterTarget,
    node: String,
    clip_request: Option<usize>,
    stage: WaterDockStage,
}

impl WaterDock {
    fn owns(&self, task: &BehaviorTaskExecutionState) -> bool {
        self.execution == task.execution_id
            && task.target() == Some(self.shore.region)
            && task.stack.len() == self.depth
            && task.is_at_or_nested_under(&self.origin, self.depth)
    }
}

pub(super) fn register(application: &mut App) {
    application.add_systems(FixedUpdate, (receive_water_docking_events, execute_water_docking)
        .chain().in_set(FixedGameSet::Think)
        .after(super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
        .before(openzt2_zoo_state::locomotion::AnimalAnimationNavigationProjection)
        .run_if(in_state(GamePhase::InGame)))
        .add_systems(PostUpdate, sample_started_docking_poses
            .after(bevy::transform::TransformSystems::Propagate)
            .run_if(in_state(GamePhase::InGame)));
}

fn receive_water_docking_events(
    mut starts: MessageReader<AnimationClipPlaybackRequestStarted>,
    mut rejections: MessageReader<AnimationClipPlaybackRequestRejected>,
    mut completed: MessageReader<AnimationCompleted>,
    mut arrivals: MessageReader<Arrived>,
    mut failures: MessageReader<NavigationFailed>,
    controllers: Query<(Entity, &openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner), With<AnimationPlaybackController>>,
    mut docks: Query<(Entity, &mut WaterDock)>,
) {
    for event in starts.read() {
        for (actor, mut dock) in &mut docks {
            if actor != event.animation_subject_entity {
                continue;
            }
            let Some(controller) = controllers
                .iter()
                .find_map(|(entity, owner)| (owner.gameplay_entity == actor).then_some(entity))
            else {
                continue;
            };
            match &mut dock.stage {
                WaterDockStage::Sampling { request, playback }
                | WaterDockStage::Finishing { request, playback }
                    if *request == event.request_id =>
                {
                    *playback = Some((controller, event.playback_generation));
                }
                _ => {}
            }
        }
    }
    for event in rejections.read() {
        for (_, mut dock) in &mut docks {
            if matches!(dock.stage, WaterDockStage::Sampling { request, .. } | WaterDockStage::Finishing { request, .. } if request == event.request_id)
            {
                dock.stage = WaterDockStage::Failed;
            }
        }
    }
    for event in completed.read() {
        for (_, mut dock) in &mut docks {
            if let WaterDockStage::Finishing {
                request,
                playback: Some((controller, generation)),
            } = dock.stage
            {
                if event.explicit_clip_request_id == Some(request)
                    && event.animation_playback_controller_entity == controller
                    && event.playback_generation == generation
                {
                    dock.stage = WaterDockStage::Finished;
                }
            }
        }
    }
    for event in arrivals.read() {
        if let Ok((_, mut dock)) = docks.get_mut(event.entity) {
            if matches!(dock.stage, WaterDockStage::Moving { request } if request == event.request_id)
                && event.target == Some(dock.shore.region)
            {
                dock.stage = WaterDockStage::Arrived;
            }
        }
    }
    for event in failures.read() {
        if let Ok((_, mut dock)) = docks.get_mut(event.entity) {
            if matches!(dock.stage, WaterDockStage::Moving { request } if request == event.request_id)
            {
                dock.stage = WaterDockStage::Failed;
            }
        }
    }
}

fn execute_water_docking(
    mut commands: Commands,
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    shores: NaturalWaterTargets,
    transforms: Query<&GlobalTransform>,
    controllers: Query<&AnimationPlaybackController>,
    movement: Query<&Destination>,
    mut sequence: ResMut<NavigationRequestSequence>,
    mut navigation: MessageWriter<DockAt>,
    mut clips: MessageWriter<AnimationClipPlaybackRequest>,
    mut cancellations: MessageWriter<CancelAnimationClipPlaybackRequest>,
    mut actors: Query<
        (
            Entity,
            Option<&mut BehaviorTaskExecutionState>,
            Option<&SelectedNaturalWaterTarget>,
            Option<&mut WaterDock>,
            Has<PendingBehaviorTaskFailure>,
            Has<WorldEntityIsCrated>,
            Has<Dead>,
            Has<Frozen>,
            Has<Tranquilized>,
        ),
        Or<(With<BehaviorTaskExecutionState>, With<WaterDock>)>,
    >,
) {
    for (actor, task, selected, pending, failing, crated, dead, frozen, tranquilized) in &mut actors
    {
        let mut task = task;
        let mut pending = pending;
        let invalid = failing || crated || dead || frozen || tranquilized;
        if let Some(dock) = pending.as_deref() {
            if invalid
                || task.as_ref().is_none_or(|task| !dock.owns(task))
                || !shores.valid(&dock.shore)
            {
                if let Some(request) = dock.clip_request {
                    cancellations.write(CancelAnimationClipPlaybackRequest {
                        animation_subject_entity: actor,
                        request_id: request,
                    });
                }
                if matches!(dock.stage, WaterDockStage::Moving { request } if movement.get(actor).is_ok_and(|destination| destination.request_id == request))
                {
                    commands.entity(actor).remove::<(Destination, Docking)>();
                }
                if !invalid && task.as_ref().is_some_and(|task| dock.owns(task)) {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                }
                commands.entity(actor).remove::<(
                    WaterDock,
                    SamplingBehaviorDockPose,
                    SelectedNaturalWaterTarget,
                )>();
                continue;
            }
        }
        let Some(task) = task
            .as_deref_mut()
            .filter(|task| !invalid && task.next_action_tick <= clock.tick)
        else {
            continue;
        };
        let Some(BehaviorAction::DockWater(action)) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, task))
        else {
            continue;
        };
        let Some(shore) = selected
            .copied()
            .filter(|shore| task.target() == Some(shore.region) && shores.valid(shore))
        else {
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
                    if !point.is_finite() || !shores.docking_endpoint(actor, point, &shore) {
                        dock.stage = WaterDockStage::Failed;
                        continue;
                    }
                    let request = sequence.next();
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
                    dock.stage = WaterDockStage::Moving { request };
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
                    advance_behavior_task_to_next_action(task, clock.tick);
                    commands
                        .entity(actor)
                        .remove::<(WaterDock, SamplingBehaviorDockPose)>();
                }
                WaterDockStage::Failed => {
                    if let Some(request) = dock.clip_request.take() {
                        cancellations.write(CancelAnimationClipPlaybackRequest {
                            animation_subject_entity: actor,
                            request_id: request,
                        });
                    }
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    commands
                        .entity(actor)
                        .remove::<(WaterDock, SamplingBehaviorDockPose)>();
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
                WaterDockStage::Moving { request } => {
                    if !movement.get(actor).is_ok_and(|destination| {
                        destination.request_id == request
                            && shores.docking_endpoint(actor, destination.world, &shore)
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
                })
                .id;
            commands.entity(actor).insert((
                SamplingBehaviorDockPose,
                WaterDock {
                    execution: task.execution_id,
                    origin: task.create_return_frame_after_action(task.action),
                    depth: task.stack.len(),
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

fn sample_started_docking_poses(
    mut commands: Commands,
    mut docks: Query<(Entity, &mut WaterDock)>,
    controllers: Query<&AnimationPlaybackController>,
    transforms: Query<&GlobalTransform>,
    children: Query<&Children>,
    nodes: super::behavior_docking_execution::NamedNodeTransforms,
    mut cancellations: MessageWriter<CancelAnimationClipPlaybackRequest>,
) {
    for (actor, mut dock) in &mut docks {
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
        let node = super::behavior_docking_execution::find_named_descendant_global_transform(
            actor, &dock.node, &children, &nodes,
        );
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
