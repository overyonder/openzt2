use super::{fence_navigation_policy::FenceNavigationPolicy, pathfinding::plan_route};
use avian3d::prelude::{LinearVelocity, RigidBody};
use bevy::prelude::*;
use openzt2_game_assets::{
    animation::animation_set_asset_types::AnimationSetAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_world_state::{
    animation_graph::{
        animation_graph_playback_message_types::{
            AnimationClipPlaybackRequest, AnimationClipPlaybackRequestRejected,
            CancelAnimationClipPlaybackRequest,
        },
        animation_presentation_relationship_types::AnimationPresentationOwner,
    },
    animation_playback::{
        animation_event_message_types::AnimationCompleted,
        animation_joint_target_types::AnimationJointTarget,
        animation_playback_controller_types::{
            AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
        },
        locomotion_root_motion::{clip_motion_velocity, ConsumedLocomotionRootMotion},
    },
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    aquatic::aquatic_simulation_types::{Tank, TankGeometry},
    behavior_task_execution_types::{
        BehaviorHeldAnimationClip, BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    habitat::{
        habitat_membership_and_containment::locate_habitat_at_world_position,
        habitat_types::{HabitatIndex, HabitatRegion},
    },
    locomotion::locomotion_types::{
        ActiveTerrainDerivedNavigationGraph, Arrived, Destination, Docking, LocomotionCapacity,
        LocomotionMode, NavAgent, NavigationFailed, NavigationFailure, NavigationOverlay,
        NavigationRequestSequence, Route, RouteScratch, Velocity,
    },
    staff::staff_employment_types::{Staff, StaffClimbingTankGate, StaffRole},
    terrain::dry_surface_eligibility::DrySurfaceEligibility,
    topology::{
        gate_operation_types::Gate,
        topology_graph_types::{FenceEdge, TopologyGrid, TopologyNode},
    },
};

use openzt2_zoo_state::staff::staff_tank_gate_types::{
    StaffTankGateJourney, StaffTankGateResume, StaffTankGateStage,
};

pub(super) fn resume_completed_staff_tank_gate_journeys(
    mut commands: Commands,
    actors: Query<(
        Entity,
        &StaffTankGateResume,
        Option<&BehaviorTaskExecutionState>,
    )>,
) {
    for (actor, resume, task) in &actors {
        commands.entity(actor).remove::<StaffTankGateResume>();
        if task.is_some_and(|task| {
            task.execution_id == resume.execution_id
                && task.stack.len() == resume.depth
                && task.is_at_or_nested_under(&resume.origin, resume.depth)
        }) {
            commands.entity(actor).insert(resume.goal);
            if let Some(docking) = resume.docking {
                commands.entity(actor).insert(docking);
            }
        }
        if let Some(body) = resume.body {
            commands.entity(actor).insert(body);
        }
    }
}

/// Splits a cross-tank destination into ordinary routes on either side of an
/// actual gate. Gate candidates are tested by the same route planner as any
/// other destination before the approach is installed.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn begin_staff_tank_gate_journeys(
    mut commands: Commands,
    graph: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    capacity: Option<Res<LocomotionCapacity>>,
    overlay: Res<NavigationOverlay>,
    fences: FenceNavigationPolicy,
    surfaces: DrySurfaceEligibility,
    habitats: Res<HabitatIndex>,
    grid: Res<TopologyGrid>,
    tanks: Query<(&TankGeometry, &HabitatRegion, &WorldMember), With<Tank>>,
    gates: Query<(&FenceEdge, &Gate)>,
    nodes: Query<&TopologyNode>,
    mut sequence: ResMut<NavigationRequestSequence>,
    mut scratch: Local<RouteScratch>,
    mut actors: Query<
        (
            Entity,
            &GlobalTransform,
            &NavAgent,
            Option<&LocomotionMode>,
            &BehaviorTaskExecutionState,
            &WorldMember,
            &mut Destination,
            &mut Route,
            Option<&Docking>,
        ),
        (
            With<Staff>,
            Without<StaffTankGateJourney>,
            Without<PendingBehaviorTaskFailure>,
            Without<openzt2_zoo_state::behavior_task_execution_types::BehaviorEvasionSteering>,
        ),
    >,
    mut failures: MessageWriter<NavigationFailed>,
) {
    let (Some(graph), Some(capacity)) = (graph, capacity.filter(|capacity| capacity.is_valid()))
    else {
        return;
    };
    let navigation = graph.terrain_derived_navigation_graph();
    scratch.ensure_capacity(
        navigation.navigation_node_count(),
        capacity.max_route_points,
    );
    for (actor, transform, agent, mode, task, world, mut destination, mut route, docking) in
        &mut actors
    {
        let position = transform.translation();
        let source_tank = locate_habitat_at_world_position(&habitats, position.xz(), *grid)
            .filter(|tank| tanks.contains(*tank));
        let target_tank =
            locate_habitat_at_world_position(&habitats, destination.world.xz(), *grid)
                .filter(|tank| tanks.contains(*tank));
        if source_tank == target_tank {
            continue;
        }
        if !agent
            .capabilities
            .contains(openzt2_zoo_state::locomotion::locomotion_types::NavFlags::AMPHIBIOUS)
            && surfaces.on_land(destination.world.xz()) == Some(false)
        {
            failures.write(NavigationFailed {
                entity: actor,
                request_id: destination.request_id,
                reason: NavigationFailure::NoRoute,
            });
            commands.entity(actor).remove::<Destination>();
            route.clear();
            continue;
        }
        let Some(tank) = source_tank.or(target_tank) else {
            continue;
        };
        let Ok((geometry, region, tank_world)) = tanks.get(tank) else {
            continue;
        };
        if tank_world.root != world.root {
            continue;
        }
        let leaving = source_tank.is_some();
        let mut selected: Option<(Entity, Vec3, Vec3, f32)> = None;
        let mut candidate_route = Route::with_capacity(capacity.max_route_points);
        for &gate_entity in &region.boundary_fence_entities {
            let Ok((edge, gate)) = gates.get(gate_entity) else {
                continue;
            };
            if gate.locked {
                continue;
            }
            let tank_gate = fences.actor_types.definitions().is_some_and(|definitions| {
                definitions
                    .find_gate_fence(edge.definition)
                    .and_then(|gate| definitions.find_object(gate.object))
                    .is_some_and(|object| {
                        object
                            .behavior_type_identifiers
                            .contains(&openzt2_game_data::AssetId::from_key("tankgate"))
                    })
            });
            if !tank_gate {
                continue;
            }
            let (Ok(a), Ok(b)) = (nodes.get(edge.a), nodes.get(edge.b)) else {
                continue;
            };
            let a = grid.cell_translation(a.cell);
            let b = grid.cell_translation(b.cell);
            let midpoint = (a + b) * 0.5;
            let tangent = (b - a).xz().normalize_or_zero();
            let normal = Vec2::new(-tangent.y, tangent.x);
            if normal == Vec2::ZERO {
                continue;
            }
            let offset = normal * (agent.radius_m + grid.spacing_m * 0.5);
            let mut first = midpoint + Vec3::new(offset.x, 0.0, offset.y);
            let mut second = midpoint - Vec3::new(offset.x, 0.0, offset.y);
            let first_inside =
                locate_habitat_at_world_position(&habitats, first.xz(), *grid) == Some(tank);
            let second_inside =
                locate_habitat_at_world_position(&habitats, second.xz(), *grid) == Some(tank);
            if first_inside == second_inside {
                continue;
            }
            if first_inside != leaving {
                std::mem::swap(&mut first, &mut second);
            }
            let height = |point: Vec3| {
                surfaces
                    .water_surface_height(point.xz())
                    .or_else(|| surfaces.ground_height(point.xz()))
            };
            let (Some(first_y), Some(second_y)) = (height(first), height(second)) else {
                continue;
            };
            first.y = first_y;
            second.y = second_y;
            if plan_route(
                navigation,
                |from, to| {
                    overlay.fence_crossings(from, to).iter().any(|edge| {
                        fences.fence_blocks_actor(
                            *edge,
                            agent.capabilities,
                            fences.actor_types.actor_types(actor),
                        )
                    })
                },
                |from, to| {
                    fences.world_segment_blocks_actor(
                        from,
                        to,
                        agent.capabilities,
                        fences.actor_types.actor_types(actor),
                    )
                },
                agent,
                mode.copied().unwrap_or_default(),
                None,
                position,
                first,
                &mut candidate_route,
                &mut scratch,
            )
            .is_err()
            {
                continue;
            }
            let distance = position.distance_squared(first);
            if selected
                .as_ref()
                .is_none_or(|(_, _, _, best)| distance < *best)
            {
                selected = Some((gate_entity, first, second, distance));
            }
        }
        let Some((gate, entry, exit, _)) = selected else {
            failures.write(NavigationFailed {
                entity: actor,
                request_id: destination.request_id,
                reason: NavigationFailure::NoRoute,
            });
            commands.entity(actor).remove::<Destination>();
            route.clear();
            continue;
        };
        let goal = *destination;
        let request = sequence.next();
        *destination = Destination {
            request_id: request,
            world: entry,
            arrival_radius_m: agent.radius_m.max(0.1),
        };
        route.clear();
        commands
            .entity(actor)
            .insert(StaffTankGateJourney {
                execution_id: task.execution_id,
                origin: task.create_return_frame_after_action(task.action),
                depth: task.stack.len(),
                gate,
                goal,
                docking: docking.copied(),
                approach_request: request,
                entry,
                exit,
                wall_height: geometry.wall_height,
                stage: StaffTankGateStage::Approach,
                clip_request: None,
                clip_completed: false,
                failed: false,
                previous_body: None,
            })
            .remove::<Docking>();
    }
}

pub(super) fn receive_staff_tank_gate_events(
    mut arrivals: MessageReader<Arrived>,
    mut failures: MessageReader<NavigationFailed>,
    mut completed: MessageReader<AnimationCompleted>,
    mut rejected: MessageReader<AnimationClipPlaybackRequestRejected>,
    mut journeys: Query<&mut StaffTankGateJourney>,
) {
    for event in arrivals.read() {
        if let Ok(mut journey) = journeys.get_mut(event.entity) {
            if journey.stage == StaffTankGateStage::Approach
                && event.request_id == journey.approach_request
            {
                journey.stage = if journey.entry.y > journey.exit.y {
                    StaffTankGateStage::GetOnDown
                } else if journey.entry.y < journey.exit.y {
                    StaffTankGateStage::GetOnUp
                } else {
                    StaffTankGateStage::Cross
                };
            }
        }
    }
    for event in failures.read() {
        if let Ok(mut journey) = journeys.get_mut(event.entity) {
            if event.request_id == journey.approach_request {
                journey.failed = true;
            }
        }
    }
    for event in completed.read() {
        for mut journey in &mut journeys {
            if journey.clip_request.is_some()
                && journey.clip_request == event.explicit_clip_request_id
            {
                journey.clip_completed = true;
            }
        }
    }
    for event in rejected.read() {
        for mut journey in &mut journeys {
            if journey.clip_request == Some(event.request_id) {
                journey.failed = true;
            }
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn execute_staff_tank_gate_journeys(
    mut commands: Commands,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    animations: Res<Assets<AnimationSetAsset>>,
    gates: Query<&Gate>,
    mut actors: Query<(
        Entity,
        &StaffRole,
        &mut Transform,
        &GlobalTransform,
        Option<&mut Velocity>,
        Option<&BehaviorTaskExecutionState>,
        Option<&Destination>,
        Has<PendingBehaviorTaskFailure>,
        &mut StaffTankGateJourney,
        Option<&mut RigidBody>,
        Option<&mut LinearVelocity>,
    )>,
    mut controllers: Query<(
        Entity,
        &AnimationPresentationOwner,
        &AnimationPlaybackController,
        Option<&mut ConsumedLocomotionRootMotion>,
    )>,
    joints: Query<(&AnimationJointTarget, &ChildOf)>,
    transforms: Query<&GlobalTransform>,
    mut requests: MessageWriter<AnimationClipPlaybackRequest>,
    mut cancellations: MessageWriter<CancelAnimationClipPlaybackRequest>,
    mut failures: MessageWriter<NavigationFailed>,
) {
    let catalogue = definitions.get(&definition_assets);
    for (
        actor,
        role,
        mut transform,
        global,
        velocity,
        task,
        destination,
        failing,
        mut journey,
        body,
        linear_velocity,
    ) in &mut actors
    {
        let owns = !failing
            && task.is_some_and(|task| {
                task.execution_id == journey.execution_id
                    && task.stack.len() == journey.depth
                    && task.is_at_or_nested_under(&journey.origin, journey.depth)
            })
            && destination
                .is_none_or(|destination| destination.request_id == journey.approach_request);
        if !owns || journey.failed || gates.get(journey.gate).is_err() {
            if let Some(request_id) = journey.clip_request {
                cancellations.write(CancelAnimationClipPlaybackRequest {
                    animation_subject_entity: actor,
                    request_id,
                });
            }
            commands.entity(actor).remove::<(
                StaffTankGateJourney,
                StaffClimbingTankGate,
                BehaviorHeldAnimationClip,
            )>();
            if let Some(body) = journey.previous_body {
                commands.entity(actor).insert(body);
            }
            for (controller, owner, playback, _) in &mut controllers {
                if owner.gameplay_entity == actor
                    && playback.explicit_clip_request_id == journey.clip_request
                {
                    commands
                        .entity(controller)
                        .remove::<ConsumedLocomotionRootMotion>();
                }
            }
            if owns {
                commands.entity(actor).remove::<Destination>();
                failures.write(NavigationFailed {
                    entity: actor,
                    request_id: journey.goal.request_id,
                    reason: NavigationFailure::NoRoute,
                });
            }
            continue;
        }
        if journey.stage == StaffTankGateStage::Approach {
            continue;
        }
        let Some(catalogue) = catalogue else {
            continue;
        };
        if journey.previous_body.is_none() {
            if let Some(mut body) = body {
                journey.previous_body = Some(*body);
                *body = RigidBody::Kinematic;
            }
        }
        if let Some(mut velocity) = velocity {
            velocity.0 = Vec3::ZERO;
        }
        if let Some(mut velocity) = linear_velocity {
            velocity.0 = Vec3::ZERO;
        }
        commands.entity(actor).insert(StaffClimbingTankGate);
        let Some(clips) = catalogue
            .find_staff(role.0)
            .and_then(|role| role.tank_gate_animations.as_ref())
        else {
            journey.failed = true;
            continue;
        };
        if journey.clip_completed {
            journey.clip_completed = false;
            let completed_request = journey.clip_request;
            journey.clip_request = None;
            for (controller, owner, playback, _) in &mut controllers {
                if owner.gameplay_entity == actor
                    && playback.explicit_clip_request_id == completed_request
                {
                    commands
                        .entity(controller)
                        .remove::<ConsumedLocomotionRootMotion>();
                }
            }
            match journey.stage {
                StaffTankGateStage::GetOnUp => journey.stage = StaffTankGateStage::ClimbUp,
                StaffTankGateStage::GetOnDown => journey.stage = StaffTankGateStage::ClimbDown,
                StaffTankGateStage::GetOffUp | StaffTankGateStage::GetOffDown => {
                    transform.translation = journey.exit;
                    commands
                        .entity(actor)
                        .insert(StaffTankGateResume {
                            execution_id: journey.execution_id,
                            origin: journey.origin.clone(),
                            depth: journey.depth,
                            goal: journey.goal,
                            docking: journey.docking,
                            body: journey.previous_body,
                        })
                        .remove::<(
                            StaffTankGateJourney,
                            StaffClimbingTankGate,
                            BehaviorHeldAnimationClip,
                        )>();
                    continue;
                }
                _ => {}
            }
        }
        let up = matches!(
            journey.stage,
            StaffTankGateStage::GetOnUp
                | StaffTankGateStage::ClimbUp
                | StaffTankGateStage::GetOffUp
        );
        let climb = matches!(
            journey.stage,
            StaffTankGateStage::ClimbUp | StaffTankGateStage::ClimbDown
        );
        let wanted = match journey.stage {
            StaffTankGateStage::Cross => &clips.walk_ahead,
            StaffTankGateStage::GetOnUp => &clips.get_on_up,
            StaffTankGateStage::ClimbUp => &clips.climb_up,
            StaffTankGateStage::GetOffUp => &clips.get_off_up,
            StaffTankGateStage::GetOnDown => &clips.get_on_down,
            StaffTankGateStage::ClimbDown => &clips.climb_down,
            StaffTankGateStage::GetOffDown => &clips.get_off_down,
            StaffTankGateStage::Approach => continue,
        };
        if journey.clip_request.is_none() {
            if journey.stage == StaffTankGateStage::GetOnUp {
                transform.translation = journey.entry;
            }
            let direction = (journey.exit - journey.entry).xz();
            transform.rotation = Quat::from_rotation_y(direction.x.atan2(direction.y));
            let request = requests.write(AnimationClipPlaybackRequest {
                animation_subject_entity: actor,
                animation_clip_asset_key: wanted.clone(),
                blend_duration_milliseconds: 0,
                playback_speed_permille: 1000,
                playback_repetition_policy: if climb || journey.stage == StaffTankGateStage::Cross {
                    AnimationPlaybackRepetitionPolicy::Loop
                } else {
                    AnimationPlaybackRepetitionPolicy::PlayOnce
                },
            });
            journey.clip_request = Some(request.id);
            commands.entity(actor).insert(BehaviorHeldAnimationClip {
                request_id: request.id,
            });
            continue;
        }
        for (controller_entity, owner, controller, motion) in &mut controllers {
            if owner.gameplay_entity != actor
                || controller.explicit_clip_request_id != journey.clip_request
            {
                continue;
            }
            let Some(clip) = animations
                .get(&controller.animation_set_asset)
                .and_then(|set| set.find_animation_clip_record_by_asset_key(wanted))
            else {
                continue;
            };
            let Some(root) = clip
                .authored_animation_clip_metadata
                .as_ref()
                .and_then(|metadata| metadata.root_motion_joint.as_ref())
            else {
                journey.failed = true;
                break;
            };
            let Some(parent) = joints.iter().find_map(|(binding, parent)| {
                (binding.animation_playback_controller_entity == controller_entity
                    && binding.joint_asset_key == root.joint_asset_key)
                    .then(|| transforms.get(parent.parent()).ok())
                    .flatten()
            }) else {
                journey.failed = true;
                break;
            };
            if clip_motion_velocity(clip, parent, 1000).is_none_or(|velocity| {
                if climb {
                    velocity.y.abs() <= f32::EPSILON
                } else if journey.stage == StaffTankGateStage::Cross {
                    velocity.xz().length_squared() <= f32::EPSILON
                } else {
                    false
                }
            }) {
                journey.failed = true;
                break;
            }
            let root_parent = global.affine().inverse() * parent.affine();
            let Some(mut motion) = motion.filter(|motion| motion.owns_playback(controller)) else {
                commands
                    .entity(controller_entity)
                    .insert(ConsumedLocomotionRootMotion::new(
                        journey.clip_request.expect("an active climb has a request"),
                        root_parent,
                    ));
                continue;
            };
            motion.update_root_parent_transform(root_parent);
            if journey.stage == StaffTankGateStage::Cross {
                let remaining = journey.exit - transform.translation;
                let step = motion.translation().xz().length();
                transform.translation +=
                    remaining.normalize_or_zero() * step.min(remaining.length());
                if transform.translation.distance_squared(journey.exit) <= 0.0001 {
                    if let Some(request_id) = journey.clip_request {
                        cancellations.write(CancelAnimationClipPlaybackRequest {
                            animation_subject_entity: actor,
                            request_id,
                        });
                    }
                    commands
                        .entity(controller_entity)
                        .remove::<ConsumedLocomotionRootMotion>();
                    commands
                        .entity(actor)
                        .insert(StaffTankGateResume {
                            execution_id: journey.execution_id,
                            origin: journey.origin.clone(),
                            depth: journey.depth,
                            goal: journey.goal,
                            docking: journey.docking,
                            body: journey.previous_body,
                        })
                        .remove::<(
                            StaffTankGateJourney,
                            StaffClimbingTankGate,
                            BehaviorHeldAnimationClip,
                        )>();
                }
                break;
            }
            if !climb {
                let delta = transform.rotation * motion.translation();
                transform.translation += delta;
                break;
            }
            let step = motion.translation().y.abs();
            let target = if up {
                (journey.wall_height - 1.75)
                    .min(journey.exit.y)
                    .max(journey.entry.y)
            } else {
                journey.exit.y
            };
            transform.translation.y = if up {
                (transform.translation.y + step).min(target)
            } else {
                (transform.translation.y - step).max(target)
            };
            if (transform.translation.y - target).abs() <= 0.001 {
                if let Some(request_id) = journey.clip_request {
                    cancellations.write(CancelAnimationClipPlaybackRequest {
                        animation_subject_entity: actor,
                        request_id,
                    });
                }
                commands
                    .entity(controller_entity)
                    .remove::<ConsumedLocomotionRootMotion>();
                journey.clip_request = None;
                journey.stage = if up {
                    StaffTankGateStage::GetOffUp
                } else {
                    StaffTankGateStage::GetOffDown
                };
            }
            break;
        }
    }
}
