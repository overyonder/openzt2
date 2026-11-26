use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::{
    animation_graph::animation_graph_playback_message_types::AnimationClipPlaybackRequest,
    animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy,
    application_schedule::FixedGameSet,
    simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    SplineDockMotion, SplineDockPhase, SplineDockRetry,
};
use openzt2_zoo_state::{
    animal_behavior::interaction_container_membership::{
        requests::{InteractionContainerAdmissionOutcome, InteractionContainerAdmissionRequest},
        DockedAtInteractionContainer, InteractionContainerMemberships,
    },
    aquatic::aquatic_simulation_types::{TankGeometry, TankWallTopHeight},
    behavior_task_execution_types::{
        running_actions::{
            BehaviorStepFixedSet, RunBehaviorDockNow, RunBehaviorDockSpline,
            RunBehaviorDockTankWall, RunBehaviorUsePortal, RunningBehaviorInstruction,
            WhenBehaviorAction,
        },
        BehaviorTaskExecutionState, PendingBehaviorDockingCompletion, PendingBehaviorTaskFailure,
    },
    habitat::habitat_types::{HabitatMember, HabitatRegion},
    locomotion::locomotion_types::{
        ActiveTerrainDerivedNavigationGraph, Arrived, DockAt, LocomotionMode, NavAgent, NavFlags,
        NavigateTo, NavigationFailed, NavigationRequestGeneration,
    },
    topology::{
        gate_operation_types::Gate,
        habitat_boundary_queries::HabitatBoundaryQueries,
        topology_graph_types::{FenceSegment, Portal, TopologyGrid, TopologyNode},
    },
};

use super::{
    behavior_container_admission::BehaviorAdmissionInstruction,
    behavior_named_nodes::NamedNodeTransforms,
    behavior_object_attachment_execution::{detach::detach_object, ContainedObject},
    behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
};

type DockActor<'w, 's, A> = Query<
    'w,
    's,
    (
        &'static BehaviorTaskExecutionState,
        &'static RunningBehaviorInstruction,
        &'static A,
        &'static WorldMember,
    ),
    Without<PendingBehaviorTaskFailure>,
>;

#[derive(SystemParam)]
struct DockPoses<'w, 's> {
    transforms: Query<'w, 's, &'static GlobalTransform>,
    nodes: NamedNodeTransforms<'w, 's>,
    animation: super::docking_animation_pose::DockingAnimationPose<'w, 's>,
}

impl DockPoses<'_, '_> {
    fn node(&self, actor: Entity, node: Option<&str>) -> Option<GlobalTransform> {
        if let Some(node) = node {
            self.nodes.pose(actor, node).copied()
        } else {
            self.transforms.get(actor).ok().copied()
        }
    }

    fn align(
        &self,
        actor: Entity,
        subject_node: Option<&str>,
        subject_animation: Option<&str>,
        target_pose: GlobalTransform,
    ) -> Option<Transform> {
        let subject = self.transforms.get(actor).ok()?;
        let node = if let Some(name) = subject_node {
            let (entity, live) = self.nodes.entity_and_pose(actor, name)?;
            subject_animation
                .and_then(|clip| self.animation.sample_node(actor, entity, clip, subject))
                .unwrap_or(*live)
        } else {
            *subject
        };
        let attachment = node.reparented_to(subject);
        let root = target_pose.affine() * attachment.compute_affine().inverse();
        root.is_finite()
            .then(|| Transform::from_matrix(Mat4::from(root)))
    }
}

pub(super) fn register(app: &mut App) {
    app.add_observer(reserve_immediate_dock)
        .add_observer(apply_immediate_dock)
        .add_observer(start_spline_dock)
        .add_observer(start_tank_wall_dock)
        .add_observer(use_portal)
        .add_observer(clear_portal_crossing)
        .add_observer(retire_spline_docking)
        .add_systems(
            FixedUpdate,
            (
                follow_docking_splines,
                follow_bound_immediate_dock_poses,
                advance_portal_crossings,
            )
                .in_set(BehaviorStepFixedSet::Think),
        )
        .add_systems(
            FixedUpdate,
            (finish_special_docks, accept_spline_approach_outcomes).in_set(FixedGameSet::Act),
        );
}

#[derive(Component)]
struct PortalCrossing {
    instruction:
        openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorInstructionId,
    gate: Entity,
    destination: Transform,
    fallback: Transform,
    disabled_collider: bool,
}

fn clear_portal_crossing(
    removed: On<Remove<RunBehaviorUsePortal>>,
    crossings: Query<&PortalCrossing>,
    mut commands: Commands,
) {
    if let Ok(crossing) = crossings.get(removed.entity) {
        let mut actor = commands.entity(removed.entity);
        if crossing.disabled_collider {
            actor.try_remove::<avian3d::prelude::ColliderDisabled>();
        }
        actor.try_remove::<PortalCrossing>();
    }
}

fn advance_portal_crossings(
    mut actors: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &PortalCrossing,
            &NavAgent,
            Option<&ChildOf>,
            &mut Transform,
        ),
        (
            With<RunBehaviorUsePortal>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
    gates: Query<&Gate>,
    parents: Query<&GlobalTransform>,
    time: Res<Time<Fixed>>,
    clock: Single<&ZooClock>,
    mut commands: Commands,
) {
    for (actor, task, step, crossing, agent, parent, mut local) in &mut actors {
        if step.id() != crossing.instruction || !step.is_current(task) {
            continue;
        }
        let parent = parent.and_then(|value| parents.get(value.parent()).ok());
        let world = parent.map_or(local.translation, |pose| {
            pose.transform_point(local.translation)
        });
        let open = gates
            .get(crossing.gate)
            .is_ok_and(|gate| gate.open && !gate.locked);
        let mut pose = if open {
            crossing.destination
        } else {
            crossing.fallback
        };
        let delta = pose.translation - world;
        let distance = delta.length();
        let travel = agent.max_speed_mps * time.delta_secs();
        let arrived = !open || distance <= travel;
        if !arrived {
            pose.translation = world + delta * (travel / distance);
        }
        local.set_if_neq(parent.map_or(pose, |parent| {
            GlobalTransform::from(pose).reparented_to(parent)
        }));
        if arrived {
            step.complete(actor, clock.tick, &mut commands);
        }
    }
}

#[derive(Component)]
#[relationship(relationship_target = ImmediateDockedActors)]
struct ImmediateDockBinding {
    #[relationship]
    target: Entity,
    subject_node: Option<String>,
    target_node: Option<String>,
    subject_animation: Option<String>,
}

#[derive(Component)]
#[relationship_target(relationship = ImmediateDockBinding)]
struct ImmediateDockedActors(Vec<Entity>);

#[derive(EntityEvent)]
struct ImmediateDockAdmission {
    entity: Entity,
    target: Entity,
    slot: Option<usize>,
}

fn reserve_immediate_dock(
    activation: On<WhenBehaviorAction<RunBehaviorDockNow>>,
    actors: DockActor<RunBehaviorDockNow>,
    targets: Query<(&WorldMember, Option<&DefinitionId>)>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    memberships: InteractionContainerMemberships,
    mut commands: Commands,
) {
    let actor = activation.entity;
    let Ok((task, instruction, action, world)) = actors.get(actor) else {
        return;
    };
    let target = task.target.filter(|target| {
        *target != actor
            && targets
                .get(*target)
                .is_ok_and(|(target_world, _)| target_world.root == world.root)
    });
    let Some(target) = target else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    if let Some(slot) = memberships.actor_slot(target, actor) {
        commands.trigger(ImmediateDockAdmission {
            entity: actor,
            target,
            slot: Some(slot),
        });
        return;
    }
    let Some(reservation) = action.0.reservation.or(instruction.reservation_tag()) else {
        commands.trigger(ImmediateDockAdmission {
            entity: actor,
            target,
            slot: None,
        });
        return;
    };
    let slots = definitions
        .get(&assets)
        .and_then(|view| view.find_object(targets.get(target).ok()?.1?.0))
        .map_or_else(Vec::new, |definition| {
            definition
                .interaction_slots
                .iter()
                .enumerate()
                .filter(|(_, slot)| !slot.is_queue && slot.reservation_tag == reservation)
                .map(|(index, slot)| (index, slot.capacity))
                .collect::<Vec<_>>()
        });
    if slots.is_empty() {
        commands.trigger(ImmediateDockAdmission {
            entity: actor,
            target,
            slot: None,
        });
        return;
    }
    let owner = BehaviorAdmissionInstruction::new(actor, task, instruction);
    commands.queue(move |world: &mut World| {
        if !owner.is_current(world) {
            return;
        }
        let request = InteractionContainerAdmissionRequest::Admit {
            container: target,
            member: actor,
            slots,
            despawn_on_refusal: false,
            queue_service_slots: Vec::new(),
        };
        if let InteractionContainerAdmissionOutcome::Granted { slot, .. } = request.commit(world) {
            world.trigger(ImmediateDockAdmission {
                entity: actor,
                target,
                slot: Some(slot),
            });
        } else {
            let mut commands = world.commands();
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
    });
}

fn apply_immediate_dock(
    admitted: On<ImmediateDockAdmission>,
    clock: Single<&ZooClock>,
    mut actors: Query<(
        &mut BehaviorTaskExecutionState,
        &RunningBehaviorInstruction,
        &RunBehaviorDockNow,
        &mut Transform,
        Option<&ChildOf>,
        &WorldMember,
        Has<ImmediateDockBinding>,
    )>,
    mut poses: ParamSet<(DockPoses, Query<&mut GlobalTransform>)>,
    memberships: InteractionContainerMemberships,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    contained: Query<(&ContainedObject, &DefinitionId, Option<&WorldMember>)>,
    mut commands: Commands,
) {
    let actor = admitted.entity;
    let Ok((mut task, instruction, action, mut local, parent, member, has_binding)) =
        actors.get_mut(actor)
    else {
        return;
    };
    if task.target != Some(admitted.target) || !instruction.is_current(&task) {
        return;
    }
    let action = &action.0;
    let target = admitted.target;
    if action.unparent && memberships.actor_slot(target, actor).is_some() {
        let release = definitions.get(&assets).and_then(|definitions| {
            let (_, id, _) = contained.get(actor).ok()?;
            let actions = &definitions.find_object(id.0)?.detach_actions;
            let rule = ["dropitem", "fallitem"]
                .into_iter()
                .map(openzt2_game_data::AssetId::from_key)
                .find(|key| actions.iter().any(|rule| rule.name == *key))?;
            Some((definitions, rule))
        });
        if let Some((definitions, rule)) = release {
            if detach_object(
                target,
                actor,
                Some(rule),
                &definitions,
                &contained,
                &poses.p0().transforms,
                &mut commands,
            ) {
                commands.entity(actor).try_remove::<ImmediateDockBinding>();
                instruction.complete(actor, clock.tick, &mut commands);
            } else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            }
            return;
        }
    }
    if !action.redock
        && !action.unparent
        && !action.parent_to_target
        && memberships.is_docked(target, actor)
    {
        instruction.complete(actor, clock.tick, &mut commands);
        return;
    }
    let (pose, parent_pose) = {
        let poses = poses.p0();
        let Some(pose) =
            poses
                .node(target, action.target_node.as_deref())
                .and_then(|target_pose| {
                    poses.align(
                        actor,
                        action.subject_node.as_deref(),
                        action.target_animation.as_deref(),
                        target_pose,
                    )
                })
        else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            return;
        };
        let parent_pose = parent
            .filter(|_| !action.unparent)
            .and_then(|parent| poses.transforms.get(parent.parent()).ok().copied());
        (pose, parent_pose)
    };
    let global = GlobalTransform::from(pose);
    *local = parent_pose.map_or(pose, |parent| global.reparented_to(&parent));
    if let Ok(mut current) = poses.p1().get_mut(actor) {
        current.set_if_neq(global);
    }
    let entity = &mut commands.entity(actor);
    if action.parent_to_target {
        entity.insert(ImmediateDockBinding {
            target,
            subject_node: action.subject_node.clone(),
            target_node: action.target_node.clone(),
            subject_animation: action.target_animation.clone(),
        });
    }
    if action.unparent {
        entity.remove::<(ImmediateDockBinding, ChildOf)>();
    }
    if let Some(slot) = admitted.slot {
        task.interaction_slot = Some(slot);
        entity.insert_if_new(DockedAtInteractionContainer);
    }
    if action.unparent && (has_binding || parent.is_some()) {
        use openzt2_zoo_state::animal_behavior::behavior_world_release_types::{
            BehaviorWorldObjectReleased, BehaviorWorldReleaseMotion,
        };
        commands.trigger(BehaviorWorldObjectReleased {
            object: actor,
            world: member.root,
            pose,
            motion: if has_binding {
                BehaviorWorldReleaseMotion::Fall { speed_mps: 0.0 }
            } else {
                BehaviorWorldReleaseMotion::Drop
            },
        });
    }
    instruction.complete(actor, clock.tick, &mut commands);
}

fn follow_bound_immediate_dock_poses(
    poses: DockPoses,
    mut actors: Query<(
        Entity,
        &ImmediateDockBinding,
        &mut Transform,
        Option<&ChildOf>,
    )>,
    mut commands: Commands,
) {
    for (actor, binding, mut transform, parent) in &mut actors {
        let Some(world_pose) = poses
            .node(binding.target, binding.target_node.as_deref())
            .and_then(|target| {
                poses.align(
                    actor,
                    binding.subject_node.as_deref(),
                    binding.subject_animation.as_deref(),
                    target,
                )
            })
        else {
            commands.entity(actor).remove::<ImmediateDockBinding>();
            continue;
        };
        let pose = parent
            .and_then(|parent| poses.transforms.get(parent.parent()).ok())
            .map_or(world_pose, |parent| {
                GlobalTransform::from(world_pose).reparented_to(parent)
            });
        transform.set_if_neq(pose);
    }
}

#[derive(SystemParam)]
struct DockNavigation<'w, 's> {
    agents: Query<'w, 's, (&'static NavAgent, &'static mut NavigationRequestGeneration)>,
    requests: MessageWriter<'w, DockAt>,
}

impl DockNavigation<'_, '_> {
    fn request(
        &mut self,
        actor: Entity,
        step: &RunningBehaviorInstruction,
        target: Entity,
        target_pose: &GlobalTransform,
        destination: Transform,
        commands: &mut Commands,
    ) {
        let Ok((agent, mut generation)) = self.agents.get_mut(actor) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, commands);
            return;
        };
        let request_id = generation.advance();
        let target_from_world = target_pose.affine().inverse();
        self.requests.write(DockAt {
            entity: actor,
            target,
            request_id,
            local_point: target_from_world.transform_point3(destination.translation),
            local_forward: target_from_world.transform_vector3(destination.rotation * Vec3::Z),
            radius_m: agent.radius_m.max(0.05),
        });
        commands
            .entity(actor)
            .insert(PendingBehaviorDockingCompletion {
                target,
                instruction: step.id(),
                request_id,
            });
    }
}

fn start_spline_dock(
    activation: On<WhenBehaviorAction<RunBehaviorDockSpline>>,
    clock: Single<&ZooClock>,
    actors: DockActor<RunBehaviorDockSpline>,
    targets: Query<&WorldMember>,
    memberships: InteractionContainerMemberships,
    poses: DockPoses,
    habitats: Query<(&HabitatMember, Option<&LocomotionMode>)>,
    mut agents: Query<(&NavAgent, &mut NavigationRequestGeneration)>,
    mut routes: MessageWriter<NavigateTo>,
    disabled: Query<(), With<avian3d::prelude::ColliderDisabled>>,
    motions: Query<&SplineDockMotion>,
    mut commands: Commands,
) {
    let actor = activation.entity;
    if motions
        .get(actor)
        .is_ok_and(|motion| motion.instruction == activation.instruction)
    {
        return;
    }
    let Ok((task, step, action, world)) = actors.get(actor) else {
        return;
    };
    let action = &action.0;
    let Some(target) = task.target.filter(|target| {
        targets
            .get(*target)
            .is_ok_and(|other| other.root == world.root)
    }) else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    if !action.redock && memberships.is_docked(target, actor) {
        step.complete(actor, clock.tick, &mut commands);
        return;
    }
    let (Some(subject), Some(pose), Ok((agent, mut generation))) = (
        poses.node(actor, None),
        poses
            .node(target, action.target_node.as_deref())
            .and_then(|node| {
                poses.align(
                    actor,
                    action.subject_node.as_deref(),
                    action.target_animation.as_deref(),
                    node,
                )
            }),
        agents.get_mut(actor),
    ) else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    let start = subject.compute_transform();
    let distance = start.translation.distance(pose.translation);
    let forward = pose.rotation * Vec3::Z;
    if distance <= agent.radius_m && (start.rotation * Vec3::Z).dot(forward) >= 0.99 {
        if memberships.actor_slot(target, actor).is_some() {
            commands
                .entity(actor)
                .insert_if_new(DockedAtInteractionContainer);
        }
        step.complete(actor, clock.tick, &mut commands);
        return;
    }
    let phase = if distance > 3.0 {
        let request_id = generation.advance();
        routes.write(NavigateTo {
            entity: actor,
            request_id,
            destination: pose.translation - 3.0 * forward,
            arrival_radius_m: agent.radius_m.max(0.05),
        });
        SplineDockPhase::Approach(request_id)
    } else {
        SplineDockPhase::Curve {
            start,
            advanced: 0.0,
        }
    };
    commands.entity(actor).insert(SplineDockMotion {
        instruction: step.id(),
        target,
        phase,
        owns_disabled_collider: action.ignore_target_collision && !disabled.contains(actor),
    });
    if matches!(phase, SplineDockPhase::Approach(_))
        && habitats
            .get(actor)
            .ok()
            .zip(habitats.get(target).ok())
            .is_some_and(|((subject, mode), (target, _))| {
                mode == Some(&LocomotionMode::Swim)
                    && subject.habitat_entity == target.habitat_entity
            })
    {
        commands.entity(actor).insert(SplineDockRetry(Timer::new(
            std::time::Duration::from_secs(2) + std::time::Duration::from_nanos(1),
            TimerMode::Once,
        )));
    }
}

fn accept_spline_approach_outcomes(
    mut arrivals: MessageReader<Arrived>,
    mut failures: MessageReader<NavigationFailed>,
    mut motions: Query<(
        &RunningBehaviorInstruction,
        &mut SplineDockMotion,
        Has<SplineDockRetry>,
    )>,
    mut commands: Commands,
) {
    for arrival in arrivals.read() {
        let Ok((step, mut motion, retry)) = motions.get_mut(arrival.entity) else {
            continue;
        };
        if motion.instruction == step.id()
            && matches!(motion.phase, SplineDockPhase::Approach(request) if request == arrival.request_id)
        {
            motion.phase = SplineDockPhase::Ready;
            if retry {
                commands.entity(arrival.entity).remove::<SplineDockRetry>();
            }
        }
    }
    for failure in failures.read() {
        if motions.get(failure.entity).is_ok_and(|(step, motion, retry)| {
            !retry && motion.instruction == step.id() && matches!(motion.phase, SplineDockPhase::Approach(request) if request == failure.request_id)
        }) {
            mark_behavior_task_for_failure_and_stop_navigation(failure.entity, &mut commands);
        }
    }
}

fn follow_docking_splines(
    clock: Single<&ZooClock>,
    time: Res<Time<Fixed>>,
    poses: DockPoses,
    buoyancy: super::docking_animation_pose::DockingBuoyancy,
    memberships: InteractionContainerMemberships,
    mut actors: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorDockSpline,
            &NavAgent,
            &mut SplineDockMotion,
            &mut Transform,
            Option<&ChildOf>,
            Option<&mut avian3d::prelude::LinearVelocity>,
            Has<avian3d::prelude::ColliderDisabled>,
            Option<(&mut SplineDockRetry, &mut NavigationRequestGeneration)>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    mut commands: Commands,
    mut routes: MessageWriter<NavigateTo>,
) {
    for (
        actor,
        task,
        step,
        action,
        agent,
        mut motion,
        mut local,
        parent,
        velocity,
        collision_disabled,
        retry,
    ) in &mut actors
    {
        if !step.can_update_after_activation(clock.tick)
            || !step.is_current(task)
            || motion.instruction != step.id()
        {
            continue;
        }
        if matches!(motion.phase, SplineDockPhase::Approach(_)) && retry.as_ref().is_none() {
            continue;
        }
        let (start, advanced) = match motion.phase {
            SplineDockPhase::Approach(_) => {
                if let Some((mut retry, mut generation)) = retry {
                    if retry.0.tick(time.delta()).is_finished() {
                        retry.0.reset();
                        let action = &action.0;
                        if let Some(end) = poses
                            .node(motion.target, action.target_node.as_deref())
                            .and_then(|pose| {
                                poses.align(
                                    actor,
                                    action.subject_node.as_deref(),
                                    action.target_animation.as_deref(),
                                    pose,
                                )
                            })
                        {
                            let request_id = generation.advance();
                            routes.write(NavigateTo {
                                entity: actor,
                                request_id,
                                destination: end.translation - 3.0 * (end.rotation * Vec3::Z),
                                arrival_radius_m: agent.radius_m.max(0.05),
                            });
                            motion.phase = SplineDockPhase::Approach(request_id);
                        } else {
                            mark_behavior_task_for_failure_and_stop_navigation(
                                actor,
                                &mut commands,
                            );
                        }
                    }
                }
                continue;
            }
            SplineDockPhase::Ready => {
                let Ok(current) = poses.transforms.get(actor) else {
                    continue;
                };
                (current.compute_transform(), 0.0)
            }
            SplineDockPhase::Curve { start, advanced } => (start, advanced),
        };
        let action = &action.0;
        let Some(end) = poses
            .node(motion.target, action.target_node.as_deref())
            .and_then(|target| {
                poses.align(
                    actor,
                    action.subject_node.as_deref(),
                    action.target_animation.as_deref(),
                    target,
                )
            })
        else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        if motion.owns_disabled_collider && !collision_disabled {
            commands
                .entity(actor)
                .insert(avian3d::prelude::ColliderDisabled);
        }
        if let Some(mut velocity) = velocity {
            velocity.0 = Vec3::ZERO;
        }
        let distance = start.translation.distance(end.translation);
        let advanced = advanced + agent.max_speed_mps * time.delta_secs();
        let fraction = if distance <= f32::EPSILON {
            1.0
        } else {
            (advanced / distance).clamp(0.0, 1.0)
        };
        let handle = distance / 3.0;
        let a = start.translation;
        let b = a + start.rotation * Vec3::Z * handle;
        let d = end.translation;
        let c = d - end.rotation * Vec3::Z * handle;
        let ab = a.lerp(b, fraction);
        let bc = b.lerp(c, fraction);
        let cd = c.lerp(d, fraction);
        let mut world = Transform {
            translation: ab.lerp(bc, fraction).lerp(bc.lerp(cd, fraction), fraction),
            rotation: start.rotation.slerp(end.rotation, fraction),
            scale: start.scale,
        };
        if action.use_buoy_nodes && fraction < 1.0 {
            if let Some(offset) = poses
                .node(actor, None)
                .and_then(|root| buoyancy.support_offset(actor, &root))
            {
                let start_height = (start.rotation * (start.scale * offset)).y;
                let end_height = (end.rotation * (end.scale * offset)).y;
                let curved_height = (world.rotation * (world.scale * offset)).y;
                world.translation.y +=
                    start_height + (end_height - start_height) * fraction - curved_height;
            }
        }
        *local = parent
            .and_then(|parent| poses.transforms.get(parent.parent()).ok())
            .map_or(world, |parent| {
                GlobalTransform::from(world).reparented_to(parent)
            });
        if fraction < 1.0 {
            motion.phase = SplineDockPhase::Curve { start, advanced };
            continue;
        }
        if memberships.actor_slot(motion.target, actor).is_some() {
            commands
                .entity(actor)
                .insert_if_new(DockedAtInteractionContainer);
        }
        step.complete(actor, clock.tick, &mut commands);
    }
}

fn retire_spline_docking(
    removed: On<Remove<RunBehaviorDockSpline>>,
    motions: Query<&SplineDockMotion>,
    destinations: Query<&openzt2_zoo_state::locomotion::locomotion_types::Destination>,
    mut commands: Commands,
) {
    let Ok(motion) = motions.get(removed.entity) else {
        return;
    };
    let mut actor = commands.entity(removed.entity);
    actor.try_remove::<(SplineDockMotion, SplineDockRetry)>();
    if motion.owns_disabled_collider {
        actor.try_remove::<avian3d::prelude::ColliderDisabled>();
    }
    if matches!(motion.phase, SplineDockPhase::Approach(request) if destinations.get(removed.entity).is_ok_and(|destination| destination.request_id == request))
    {
        actor.try_remove::<openzt2_zoo_state::locomotion::locomotion_types::Destination>();
    }
}

fn start_tank_wall_dock(
    activation: On<WhenBehaviorAction<RunBehaviorDockTankWall>>,
    actors: DockActor<RunBehaviorDockTankWall>,
    habitats: Query<&HabitatMember>,
    tanks: Query<(&TankGeometry, &WorldMember)>,
    boundaries: HabitatBoundaryQueries,
    nodes: Query<&TopologyNode>,
    grid: Single<&TopologyGrid>,
    walls: Query<(FenceSegment, &TankWallTopHeight)>,
    poses: DockPoses,
    pending: Query<(), With<PendingBehaviorDockingCompletion>>,
    mut navigation: DockNavigation,
    mut commands: Commands,
) {
    let actor = activation.entity;
    if pending.contains(actor) {
        return;
    }
    let Ok((task, step, action, world)) = actors.get(actor) else {
        return;
    };
    let action = &action.0;
    let tank = task
        .target
        .filter(|&target| tanks.contains(target))
        .or_else(|| habitats.get(actor).ok().map(|member| member.habitat_entity));
    let Some((tank, geometry, origin)) = tank.and_then(|tank| {
        let (geometry, tank_world) = tanks.get(tank).ok()?;
        (tank_world.root == world.root).then_some((
            tank,
            geometry,
            poses.node(actor, None)?.translation(),
        ))
    }) else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    let nearest = boundaries
        .fences(tank)
        .filter_map(|wall| {
            let (edge, height) = walls.get(wall).ok()?;
            let a = grid.cell_translation(nodes.get(edge.a()).ok()?.cell).xz();
            let b = grid.cell_translation(nodes.get(edge.b()).ok()?.cell).xz();
            let along = b - a;
            if along.length_squared() <= f32::EPSILON {
                return None;
            }
            let fraction = ((origin.xz() - a).dot(along) / along.length_squared()).clamp(0.0, 1.0);
            let point = a + fraction * along;
            Some((wall, point, height.0))
        })
        .min_by(|left, right| {
            left.1
                .distance_squared(origin.xz())
                .total_cmp(&right.1.distance_squared(origin.xz()))
                .then_with(|| left.0.cmp(&right.0))
        });
    let Some((wall, point, wall_top)) = nearest else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    let mut pose = Transform::from_translation(Vec3::new(
        point.x,
        action
            .depth_below_water
            .map_or(
                geometry.wall_base_height + action.height_above_ground.unwrap_or_default(),
                |depth| geometry.water_height - depth,
            )
            .clamp(geometry.floor_height, wall_top.max(geometry.floor_height)),
        point.y,
    ));
    let heading = (point - origin.xz()).normalize_or_zero();
    if heading != Vec2::ZERO {
        pose.rotation = Quat::from_rotation_y(heading.x.atan2(heading.y));
    }
    let Some(aligned) = poses.align(
        actor,
        action.subject_node.as_deref(),
        None,
        GlobalTransform::from(pose),
    ) else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    let Ok(wall_pose) = poses.transforms.get(wall) else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    navigation.request(actor, step, wall, wall_pose, aligned, &mut commands);
}

#[derive(SystemParam)]
struct TankPortalDocks<'w, 's> {
    definitions: Res<'w, WorldDefinitions>,
    assets: Res<'w, Assets<WorldDefinitionAsset>>,
    grid: Single<'w, 's, &'static TopologyGrid>,
    fences: Query<'w, 's, (FenceSegment, &'static Gate, &'static WorldMember)>,
    nodes: Query<'w, 's, &'static TopologyNode>,
    bounds: HabitatBoundaryQueries<'w, 's>,
    tanks: Query<'w, 's, (&'static TankGeometry, &'static HabitatRegion)>,
    ground: openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility<'w, 's>,
    navigation: Query<
        'w,
        's,
        (
            &'static ActiveTerrainDerivedNavigationGraph,
            Option<&'static openzt2_zoo_state::aquatic::tank_navigation_water::TankNavigationWater>,
        ),
    >,
}

impl TankPortalDocks<'_, '_> {
    fn endpoints(
        &self,
        gate: Entity,
        root: Entity,
        from: Vec3,
        agent: &NavAgent,
    ) -> Option<(Transform, Transform)> {
        let (fence, state, world) = self.fences.get(gate).ok()?;
        if world.root != root || !state.open || state.locked {
            return None;
        }
        let definitions = self.definitions.get(&self.assets)?;
        let definition = definitions
            .find_gate_fence(fence.definition)
            .or_else(|| definitions.find_fence(fence.definition))?;
        let portal = definitions.find_object(definition.object)?;
        if !portal
            .behavior_type_identifiers
            .contains(&openzt2_game_data::AssetId::from_key("tankportalsegment"))
        {
            return None;
        }
        let endpoint = |entity| {
            self.nodes
                .get(entity)
                .ok()
                .map(|node| self.grid.cell_translation(node.cell).xz())
        };
        let (a, b) = (endpoint(fence.a())?, endpoint(fence.b())?);
        let centre = (a + b) * 0.5;
        let tangent = (b - a).normalize_or_zero();
        if tangent == Vec2::ZERO {
            return None;
        }
        let normal = Vec2::new(-tangent.y, tangent.x);
        let forward = if (from.xz() - centre).dot(normal) > 0.0 {
            -normal
        } else {
            normal
        };
        let mut depths = [None, None];
        for (geometry, habitat) in self
            .bounds
            .habitats(gate)
            .filter_map(|tank| self.tanks.get(tank).ok())
        {
            let side = habitat
                .topology_cells
                .iter()
                .map(|cell| {
                    self.grid.cell_translation(cell.extend(0)).xz()
                        + Vec2::splat(self.grid.spacing_m * 0.5)
                })
                .min_by(|a, b| {
                    a.distance_squared(centre)
                        .total_cmp(&b.distance_squared(centre))
                })?;
            let index = usize::from((side - centre).dot(forward) > 0.0);
            depths[index] = Some(*geometry);
        }
        let [Some(near), Some(far)] = depths else {
            return None;
        };
        let property = |name| {
            let key = openzt2_game_data::AssetId::from_key(name);
            let properties = &portal.authored_runtime.numeric_behavior_properties;
            properties
                .binary_search_by_key(&key, |(id, _)| *id)
                .ok()
                .map(|index| properties[index].1)
        };
        let raised_base = near.water_height.min(far.water_height)
            - property("gateheight")?
            - property("watersurfaceoffset")?;
        let wall_ground = [a, b]
            .into_iter()
            .filter_map(|point| self.ground.ground_height(root, point))
            .reduce(f32::max)?;
        let depth = raised_base.max(wall_ground) + 1.0;
        let dock = |point: Vec2, heading: Vec2| {
            Transform::from_xyz(point.x, depth, point.y)
                .with_rotation(Quat::from_rotation_y(heading.x.atan2(heading.y)))
        };
        let first = dock(centre - forward, forward);
        let second = dock(centre + forward, -forward);
        let passable = |point: Vec3| {
            self.navigation.get(root).is_ok_and(|(graph, water)| {
                let graph = graph.terrain_derived_navigation_graph();
                graph
                    .navigation_surface_at_world_position(point)
                    .and_then(|index| graph.effective_navigation_node(index, water))
                    .is_some_and(|node| {
                        node.eligibility_flags.0 & agent.capabilities.0 & NavFlags::ELIGIBILITY_BITS
                            != 0
                            && !node
                                .eligibility_flags
                                .contains(NavFlags::DISABLED_BY_DEFAULT)
                            && f32::from(node.clearance_centimetres) >= agent.radius_m * 100.0
                    })
            })
        };
        let start_on_far_side = !passable(first.translation) && passable(second.translation);
        let (start, mut end) = if start_on_far_side {
            (second, first)
        } else {
            (first, second)
        };
        end.rotation = start.rotation;
        Some((start, end))
    }
}

fn use_portal(
    activation: On<WhenBehaviorAction<RunBehaviorUsePortal>>,
    actors: DockActor<RunBehaviorUsePortal>,
    portals: Query<(&Portal, &WorldMember, Option<&Gate>)>,
    tank_portals: TankPortalDocks,
    poses: DockPoses,
    pending: Query<(), With<PendingBehaviorDockingCompletion>>,
    agents: Query<&NavAgent>,
    mut navigation: DockNavigation,
    mut commands: Commands,
) {
    let actor = activation.entity;
    if pending.contains(actor) {
        return;
    }
    let Ok((task, step, _, world)) = actors.get(actor) else {
        return;
    };
    let Some(entrance) = task.target else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    let graph_portal = portals.get(entrance).ok().filter(|(portal, member, gate)| {
        member.root == world.root
            && portal.destination != entrance
            && gate.is_none_or(|gate| gate.open && !gate.locked)
    });
    let tank_endpoints = poses.node(actor, None).and_then(|from| {
        tank_portals.endpoints(
            entrance,
            world.root,
            from.translation(),
            agents.get(actor).ok()?,
        )
    });
    let place = poses.node(entrance, None).or_else(|| {
        let pose = tank_endpoints.map(|(near, _)| near).or_else(|| {
            let (portal, _, _) = graph_portal?;
            tank_portals.graph_endpoint_pose(entrance, portal.destination)
        })?;
        let global = GlobalTransform::from(pose);
        commands.entity(entrance).insert((pose, global));
        Some(global)
    });
    let Some(place) = place else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    let destination = if graph_portal.is_some() {
        Some(place.compute_transform())
    } else {
        tank_endpoints.map(|(near, _)| near)
    };
    if let Some(destination) = destination {
        navigation.request(actor, step, entrance, &place, destination, &mut commands);
    } else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
    }
}

impl TankPortalDocks<'_, '_> {
    fn graph_endpoint_pose(&self, source: Entity, destination: Entity) -> Option<Transform> {
        let position = self
            .grid
            .cell_translation(self.nodes.get(source).ok()?.cell);
        let destination = self
            .grid
            .cell_translation(self.nodes.get(destination).ok()?.cell);
        let forward = (destination - position).xz().try_normalize()?;
        Some(
            Transform::from_translation(position)
                .with_rotation(Quat::from_rotation_y(forward.x.atan2(forward.y))),
        )
    }
}

fn finish_special_docks(
    mut arrivals: MessageReader<Arrived>,
    mut failures: MessageReader<NavigationFailed>,
    clock: Single<&ZooClock>,
    docks: Query<
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &PendingBehaviorDockingCompletion,
            &WorldMember,
            Option<&RunBehaviorUsePortal>,
            Option<&PortalCrossing>,
        ),
        (
            Without<PendingBehaviorTaskFailure>,
            Or<(With<RunBehaviorDockTankWall>, With<RunBehaviorUsePortal>)>,
        ),
    >,
    portals: Query<(&Portal, &WorldMember, Option<&Gate>)>,
    tank_portals: TankPortalDocks,
    transforms: Query<&GlobalTransform>,
    members: Query<&WorldMember>,
    mut actors: Query<(
        &mut Transform,
        Option<&ChildOf>,
        &NavAgent,
        Has<avian3d::prelude::ColliderDisabled>,
    )>,
    mut navigation: DockNavigation,
    mut clips: MessageWriter<AnimationClipPlaybackRequest>,
    mut commands: Commands,
) {
    let portal_exit = |entrance, root| {
        let (portal, member, gate) = portals.get(entrance).ok()?;
        if member.root != root || gate.is_some_and(|gate| !gate.open || gate.locked) {
            return None;
        }
        if members.get(portal.destination).ok()?.root != root {
            return None;
        }
        let pose = transforms
            .get(portal.destination)
            .ok()
            .copied()
            .or_else(|| {
                tank_portals
                    .graph_endpoint_pose(portal.destination, entrance)
                    .map(|pose| {
                        GlobalTransform::from(pose.with_rotation(
                            pose.rotation * Quat::from_rotation_y(std::f32::consts::PI),
                        ))
                    })
            })?;
        Some((portal.destination, pose))
    };

    let mut settled_failures = bevy::ecs::entity::EntityHashSet::default();
    for event in failures.read() {
        let Ok((task, step, waiting, world, portal, crossing)) = docks.get(event.entity) else {
            continue;
        };
        if event.request_id != waiting.request_id || !waiting.owns(task, step, Some(waiting.target))
        {
            continue;
        }
        if !settled_failures.insert(event.entity) {
            continue;
        }
        let graph_exit = portal
            .filter(|_| crossing.is_none())
            .and_then(|_| portal_exit(task.target?, world.root));
        if portal.is_some() {
            if let Ok((mut transform, parent, agent, _)) = actors.get_mut(event.entity) {
                let parent = parent.and_then(|parent| transforms.get(parent.parent()).ok());
                let from = parent.map_or(transform.translation, |pose| {
                    pose.transform_point(transform.translation)
                });
                let fallback = tank_portals
                    .endpoints(waiting.target, world.root, from, agent)
                    .map(|(near, _)| GlobalTransform::from(near))
                    .or(graph_exit.map(|(_, pose)| pose));
                let Some(pose) = fallback else {
                    mark_behavior_task_for_failure_and_stop_navigation(event.entity, &mut commands);
                    continue;
                };
                transform.set_if_neq(parent.map_or_else(
                    || pose.compute_transform(),
                    |parent| pose.reparented_to(parent),
                ));
                commands
                    .entity(event.entity)
                    .remove::<PendingBehaviorDockingCompletion>();
                step.complete(event.entity, clock.tick, &mut commands);
                continue;
            }
        }
        mark_behavior_task_for_failure_and_stop_navigation(event.entity, &mut commands);
    }
    for event in arrivals.read() {
        if settled_failures.contains(&event.entity) {
            continue;
        }
        let Ok((task, step, waiting, world, portal, crossing)) = docks.get(event.entity) else {
            continue;
        };
        if event.request_id != waiting.request_id || !waiting.owns(task, step, event.target) {
            continue;
        }
        commands
            .entity(event.entity)
            .remove::<PendingBehaviorDockingCompletion>();
        if crossing.is_some() {
            continue;
        }
        if let Some(portal) = portal.filter(|_| task.target == Some(waiting.target)) {
            if let Some(clip) = &portal.0.animation {
                clips.write(AnimationClipPlaybackRequest {
                    animation_subject_entity: event.entity,
                    animation_clip_asset_key: clip.clone(),
                    blend_duration_milliseconds: 0,
                    playback_speed_permille: 1000,
                    playback_repetition_policy:
                        AnimationPlaybackRepetitionPolicy::UseAuthoredClipPolicy,
                    action_settings: None,
                });
            }
            if let Some((exit, exit_pose)) = portal_exit(waiting.target, world.root) {
                if !transforms.contains(exit) {
                    commands
                        .entity(exit)
                        .insert((exit_pose.compute_transform(), exit_pose));
                }
                navigation.request(
                    event.entity,
                    step,
                    exit,
                    &exit_pose,
                    exit_pose.compute_transform(),
                    &mut commands,
                );
                continue;
            }
            if let Ok((transform, parent, agent, collider_already_disabled)) =
                actors.get(event.entity)
            {
                let parent = parent.and_then(|parent| transforms.get(parent.parent()).ok());
                let from = parent.map_or(transform.translation, |parent| {
                    parent.transform_point(transform.translation)
                });
                if let Some((near, exit)) =
                    tank_portals.endpoints(waiting.target, world.root, from, agent)
                {
                    let mut actor = commands.entity(event.entity);
                    if !collider_already_disabled {
                        actor.insert(avian3d::prelude::ColliderDisabled);
                    }
                    actor.insert(PortalCrossing {
                        instruction: step.id(),
                        gate: waiting.target,
                        destination: exit,
                        fallback: near,
                        disabled_collider: !collider_already_disabled,
                    });
                    continue;
                }
            }
            mark_behavior_task_for_failure_and_stop_navigation(event.entity, &mut commands);
        } else {
            step.complete(event.entity, clock.tick, &mut commands);
        }
    }
}
