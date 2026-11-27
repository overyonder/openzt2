use super::locomotion_clip_dependencies::GaitRigJoints;
use super::{fence_navigation_policy::FenceNavigationPolicy, pathfinding::plan_route};
use avian3d::prelude::{LinearVelocity, RigidBody};
use bevy::prelude::*;
use openzt2_game_assets::{
    animation::{
        animation_clip_source_queries::AnimationClipSources,
        animation_set_asset_types::AnimationSetAsset,
    },
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_world_state::{
    animation_graph::{
        animation_graph_playback_message_types::{
            AnimationClipPlaybackRequest, AnimationClipPlaybackRequestRejected,
            CancelAnimationClipPlaybackRequest,
        },
        animation_presentation_relationship_types::{
            AnimationPresentationOwner, AnimationPresentations,
        },
    },
    animation_playback::{
        animation_event_message_types::AnimationCompleted,
        animation_playback_controller_types::{
            AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
        },
        locomotion_root_motion::{clip_motion_velocity, ConsumedLocomotionRootMotion},
    },
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::aquatic::tank_navigation_water::TankNavigationWater;
use openzt2_zoo_state::topology::habitat_boundary_queries::HabitatBoundaryQueries;
use openzt2_zoo_state::topology::topology_graph_types::FenceSegment;
use openzt2_zoo_state::{
    aquatic::aquatic_simulation_types::{Tank, TankGeometry},
    behavior_task_execution_types::{
        BehaviorHeldAnimationClip, BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    habitat::{
        habitat_membership_and_containment::locate_habitat_at_world_position,
        habitat_types::{HabitatCellOf, HabitatIndex},
    },
    locomotion::locomotion_types::{
        ActiveTerrainDerivedNavigationGraph, Arrived, Destination, Docking, LocomotionCapacity,
        LocomotionMode, NavAgent, NavigationFailed, NavigationFailure, NavigationRequestGeneration,
        Route, RouteScratch, Velocity,
    },
    staff::staff_employment_types::{Staff, StaffClimbingTankGate, StaffRole},
    terrain::dry_surface_eligibility::DrySurfaceEligibility,
    topology::{
        gate_operation_types::Gate,
        topology_graph_types::{TopologyGrid, TopologyNode},
    },
};

use openzt2_zoo_state::{
    behavior_task_execution_types::running_actions::{
        BehaviorInstructionId, RunningBehaviorInstruction,
    },
    staff::staff_tank_gate_types::StaffTankGateStage,
};

#[derive(Component)]
pub(super) struct TankGateJourney {
    instruction: BehaviorInstructionId,
    gate: Entity,
    goal: Destination,
    docking: Option<Docking>,
    approach_request: u64,
    entry: Vec3,
    exit: Vec3,
    wall_height: f32,
    stage: StaffTankGateStage,
    clip_request: Option<usize>,
    clip_completed: bool,
    failed: bool,
    previous_body: Option<RigidBody>,
}

#[derive(Component)]
pub(super) struct TankGateResume {
    instruction: BehaviorInstructionId,
    goal: Destination,
    docking: Option<Docking>,
    body: Option<RigidBody>,
}

use super::fence_navigation_crossing_types::FenceNavigationCrossingQueries;

pub(super) fn resume_completed_staff_tank_gate_journeys(
    mut commands: Commands,
    mut actors: Query<(
        Entity,
        &TankGateResume,
        Option<(&BehaviorTaskExecutionState, &RunningBehaviorInstruction)>,
        Option<&mut Destination>,
        Option<&mut Docking>,
        Option<&RigidBody>,
    )>,
) {
    for (actor, resume, task, destination, docking, body) in &mut actors {
        commands.entity(actor).remove::<TankGateResume>();
        if task.is_some_and(|(task, step)| resume.instruction == step.id() && step.is_current(task))
        {
            if let Some(mut destination) = destination {
                destination.set_if_neq(resume.goal);
            } else {
                commands.entity(actor).insert(resume.goal);
            }
            if let Some(saved_docking) = resume.docking {
                if let Some(mut docking) = docking {
                    docking.set_if_neq(saved_docking);
                } else {
                    commands.entity(actor).insert(saved_docking);
                }
            }
        }
        if let Some(original) = resume.body.filter(|original| body != Some(original)) {
            commands.entity(actor).insert(original);
        }
    }
}

/// Splits a cross-tank destination into ordinary routes on either side of an
/// actual gate. Gate candidates are tested by the same route planner as any
/// other destination before the approach is installed.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn begin_staff_tank_gate_journeys(
    mut commands: Commands,
    roots: Query<
        (
            &ActiveTerrainDerivedNavigationGraph,
            Option<&TankNavigationWater>,
            &LocomotionCapacity,
            &HabitatIndex,
            &TopologyGrid,
            &openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex,
        ),
        With<WorldRoot>,
    >,
    fence_crossing_index: FenceNavigationCrossingQueries,
    cell_owners: Query<&HabitatCellOf>,
    fences: FenceNavigationPolicy,
    surfaces: DrySurfaceEligibility,
    gates: Query<(FenceSegment, &Gate)>,
    boundaries: HabitatBoundaryQueries,
    mut navigation_generations: Query<&mut NavigationRequestGeneration>,
    (mut scratch, mut candidate_route): (Local<RouteScratch>, Local<Option<Route>>),
    (terrain_assets, terrain_chunks, tanks, nodes): (
        Res<Assets<openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset>>,
        Query<(&openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk,
            Option<&openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples>)>,
        Query<(&TankGeometry, &WorldMember), With<Tank>>,
        Query<&TopologyNode>,
    ),
    mut actors: Query<
        (
            Entity,
            &GlobalTransform,
            &NavAgent,
            Option<&LocomotionMode>,
            (&BehaviorTaskExecutionState, &RunningBehaviorInstruction),
            &WorldMember,
            &mut Destination,
            &mut Route,
            Option<&Docking>,
            Option<&super::authored_tank_gate_exit::AuthoredTankGateExit>,
        ),
        (
            With<Staff>,
            Without<TankGateJourney>,
            Without<PendingBehaviorTaskFailure>,
            Without<openzt2_zoo_state::behavior_task_execution_types::BehaviorEvasionSteering>,
        ),
    >,
    mut failures: MessageWriter<NavigationFailed>,
) {
    for (
        actor,
        transform,
        agent,
        mode,
        (task, step),
        world,
        mut destination,
        mut route,
        docking,
        authored,
    ) in &mut actors
    {
        let Ok((graph, tank_water, capacity, habitats, grid, terrain_index)) =
            roots.get(world.root)
        else {
            continue;
        };
        if !capacity.is_valid() {
            continue;
        }
        let navigation = graph.terrain_derived_navigation_graph();
        scratch.ensure_capacity(
            navigation.navigation_node_count(),
            capacity.max_route_points,
        );
        let candidate_route = candidate_route.get_or_insert_with(|| Route::with_capacity(0));
        if !step.is_current(task) {
            continue;
        }
        let position = transform.translation();
        let source_tank =
            locate_habitat_at_world_position(habitats, &cell_owners, position.xz(), *grid)
                .filter(|tank| tanks.contains(*tank));
        let target_tank =
            locate_habitat_at_world_position(habitats, &cell_owners, destination.world.xz(), *grid)
                .filter(|tank| tanks.contains(*tank));
        if source_tank == target_tank && authored.is_none() {
            continue;
        }
        if !agent
            .capabilities
            .contains(openzt2_zoo_state::locomotion::locomotion_types::NavFlags::AMPHIBIOUS)
            && surfaces.on_land(world.root, destination.world.xz()) == Some(false)
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
        let Some(tank) = authored
            .map(|exit| exit.tank)
            .or(source_tank)
            .or(target_tank)
        else {
            continue;
        };
        let Ok((geometry, tank_world)) = tanks.get(tank) else {
            continue;
        };
        if tank_world.root != world.root {
            continue;
        }
        let leaving = source_tank.is_some();
        let mut selected: Option<(Entity, Vec3, Vec3, f32)> = None;

        let selected_gates = authored.map(|exit| exit.gate).into_iter().chain(
            boundaries
                .fences(tank)
                .take(if authored.is_some() { 0 } else { usize::MAX }),
        );
        for gate_entity in selected_gates {
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
            let (first, second) = if let Some(exit) = authored {
                (exit.entry, exit.exit)
            } else {
                let (Ok(a), Ok(b)) = (nodes.get(edge.a()), nodes.get(edge.b())) else {
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
                    locate_habitat_at_world_position(habitats, &cell_owners, first.xz(), *grid)
                        == Some(tank);
                let second_inside =
                    locate_habitat_at_world_position(habitats, &cell_owners, second.xz(), *grid)
                        == Some(tank);
                if first_inside == second_inside {
                    continue;
                }
                if first_inside != leaving {
                    std::mem::swap(&mut first, &mut second);
                }
                let height = |point: Vec3| {
                    surfaces
                        .water_surface_height(world.root, point.xz())
                        .or_else(|| surfaces.ground_height(world.root, point.xz()))
                };
                let (Some(first_y), Some(second_y)) = (height(first), height(second)) else {
                    continue;
                };
                first.y = first_y;
                second.y = second_y;
                (first, second)
            };
            let distance = position.distance_squared(first);
            if selected
                .as_ref()
                .is_some_and(|(_, _, _, best)| distance >= *best)
            {
                continue;
            }
            if plan_route(
                navigation,
                tank_water,
                |from, to| {
                    fence_crossing_index
                        .crossing_fences(world.root, from, to)
                        .any(|edge| {
                            fences.fence_blocks_actor(
                                edge,
                                agent.capabilities,
                                fences.actor_types.actor_types(actor),
                            )
                        })
                },
                |from, to| {
                    fences.world_segment_blocks_actor(
                        world.root,
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
                candidate_route,
                &mut scratch,
                crate::locomotion::pathfinding::PRIORITY_PATH_ITERATIONS,
            )
            .is_err()
            {
                continue;
            }
            selected = Some((gate_entity, first, second, distance));
            route.clone_from(candidate_route);
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
        let Ok(mut generation) = navigation_generations.get_mut(actor) else {
            continue;
        };
        let request = generation.advance();
        *destination = Destination {
            request_id: request,
            world: entry,
            arrival_radius_m: agent.radius_m.max(0.1),
        };
        if agent
            .capabilities
            .contains(openzt2_zoo_state::locomotion::locomotion_types::NavFlags::AMPHIBIOUS)
        {
            for point in &mut route.points {
                super::spatial_index_and_docking_operations::fit_to_terrain(
                    LocomotionMode::Swim,
                    point,
                    terrain_index,
                    &terrain_assets,
                    &terrain_chunks,
                );
            }
        }
        commands
            .entity(actor)
            .insert(
                openzt2_zoo_state::locomotion::locomotion_types::RestoredNavigationRoute {
                    request_id: request,
                },
            )
            .insert(TankGateJourney {
                instruction: step.id(),
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
    owners: Query<&AnimationPresentationOwner>,
    mut journeys: Query<&mut TankGateJourney>,
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
        let Some(request_id) = event.explicit_clip_request_id else {
            continue;
        };
        let controller = event.animation_playback_controller_entity;
        let actor = owners
            .get(controller)
            .map_or(controller, |owner| owner.gameplay_entity);
        if let Ok(mut journey) = journeys.get_mut(actor) {
            if journey.clip_request == Some(request_id) {
                journey.clip_completed = true;
            }
        }
    }
    for event in rejected.read() {
        if let Ok(mut journey) = journeys.get_mut(event.animation_subject_entity) {
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
    clip_sources: AnimationClipSources,
    gates: Query<&Gate>,
    mut actors: Query<(
        Entity,
        &StaffRole,
        &mut Transform,
        &GlobalTransform,
        Option<&mut Velocity>,
        Option<(&BehaviorTaskExecutionState, &RunningBehaviorInstruction)>,
        Option<&Destination>,
        Has<PendingBehaviorTaskFailure>,
        &mut TankGateJourney,
        Option<&RigidBody>,
        Option<&mut LinearVelocity>,
    )>,
    presentations: Query<&AnimationPresentations>,
    mut controllers: Query<(
        &AnimationPlaybackController,
        Option<&GaitRigJoints>,
        Option<&mut ConsumedLocomotionRootMotion>,
    )>,
    parents: Query<&ChildOf>,
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
            && task.is_some_and(|(task, step)| {
                journey.instruction == step.id() && step.is_current(task)
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
                TankGateJourney,
                StaffClimbingTankGate,
                BehaviorHeldAnimationClip,
            )>();
            if let Some(original) = journey
                .previous_body
                .filter(|original| body != Some(original))
            {
                commands.entity(actor).insert(original);
            }
            for controller in
                AnimationPresentations::for_subject(actor, presentations.get(actor).ok())
            {
                if controllers.get(controller).is_ok_and(|(playback, _, _)| {
                    playback.explicit_clip_request_id == journey.clip_request
                }) {
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
            if let Some(body) = body {
                journey.previous_body = Some(*body);
                if *body != RigidBody::Kinematic {
                    commands.entity(actor).insert(RigidBody::Kinematic);
                }
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
            for controller in
                AnimationPresentations::for_subject(actor, presentations.get(actor).ok())
            {
                if controllers.get(controller).is_ok_and(|(playback, _, _)| {
                    playback.explicit_clip_request_id == completed_request
                }) {
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
                        .insert(TankGateResume {
                            instruction: journey.instruction,
                            goal: journey.goal,
                            docking: journey.docking,
                            body: journey.previous_body,
                        })
                        .remove::<(
                            TankGateJourney,
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
                action_settings: None,
            });
            journey.clip_request = Some(request.id);
            commands.entity(actor).insert(BehaviorHeldAnimationClip {
                request_id: request.id,
            });
            continue;
        }
        for controller_entity in
            AnimationPresentations::for_subject(actor, presentations.get(actor).ok())
        {
            let Ok((controller, rig, motion)) = controllers.get_mut(controller_entity) else {
                continue;
            };
            if controller.explicit_clip_request_id != journey.clip_request {
                continue;
            }
            let Some(clip) = animations
                .get(&controller.animation_set_asset)
                .and_then(|set| clip_sources.get(set, wanted))
            else {
                continue;
            };
            let Some(root) = clip
                .metadata
                .and_then(|metadata| metadata.root_motion_joint.as_ref())
            else {
                journey.failed = true;
                break;
            };
            let joint = root.joint_id();
            let Some(parent) = rig.and_then(|rig| rig.root_parent(joint, &parents, &transforms))
            else {
                journey.failed = true;
                break;
            };
            if clip_motion_velocity(&clip, parent, 1000).is_none_or(|velocity| {
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
            let Some(mut motion) = motion else {
                commands
                    .entity(controller_entity)
                    .insert(ConsumedLocomotionRootMotion::new(
                        journey.clip_request.expect("an active climb has a request"),
                        root_parent,
                    ));
                continue;
            };
            if !motion.owns_playback(controller) {
                *motion = ConsumedLocomotionRootMotion::new(
                    journey.clip_request.expect("an active climb has a request"),
                    root_parent,
                );
                continue;
            }
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
                        .insert(TankGateResume {
                            instruction: journey.instruction,
                            goal: journey.goal,
                            docking: journey.docking,
                            body: journey.previous_body,
                        })
                        .remove::<(
                            TankGateJourney,
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
                journey.wall_height - 1.75
            } else {
                journey.exit.y
            };
            transform.translation.y = if up {
                transform.translation.y + step
            } else {
                transform.translation.y - step
            };
            if climb_crossed_threshold(up, transform.translation.y, target) {
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

fn climb_crossed_threshold(up: bool, height: f32, target: f32) -> bool {
    if up {
        height > target
    } else {
        height < target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn climbing_remains_active_at_the_tank_height_threshold() {
        assert!(!climb_crossed_threshold(true, 4.25, 6.0 - 1.75));
        assert!(climb_crossed_threshold(true, 4.26, 6.0 - 1.75));
        assert!(!climb_crossed_threshold(false, 2.0, 2.0));
        assert!(climb_crossed_threshold(false, 1.99, 2.0));
    }
}
