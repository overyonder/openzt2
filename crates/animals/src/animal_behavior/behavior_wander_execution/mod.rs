use bevy::prelude::*;
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::behavior::action::wander::{
    BehaviorGroundLocomotionSpeed, BehaviorWanderAction,
};
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_entity_crating::{
    AnimalCrate, ScriptedCrating, UncratedWorldEntity,
};
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use openzt2_zoo_state::animal_health::types::{Dead, Tranquilized};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorEvasion, RunBehaviorWander, RunningBehaviorInstruction,
};
use openzt2_zoo_state::{
    animal_behavior::behavior_random_stream_state::BehaviorRandomStream,
    animal_lifecycle::types::Animal,
    behavior_task_execution_types::{BehaviorTaskExecutionState, PendingBehaviorTaskFailure},
    habitat::{
        habitat_membership_and_containment::locate_habitat_at_world_position,
        habitat_types::HabitatIndex,
    },
    locomotion::locomotion_types::{
        ActiveTerrainDerivedNavigationGraph, Arrived, Destination, LocomotionMode, NavAgent,
        NavFlags, NavigateTo, NavigationFailed, NavigationRequestGeneration, Route,
    },
    staff::staff_employment_types::{Staff, StaffSubmergedDepth},
    topology::topology_graph_types::TopologyGrid,
};

use openzt2_zoo_state::behavior_task_execution_types::{
    BehaviorEvasionNavigation, BehaviorEvasionSteering, BehaviorWanderNavigation,
};

use super::behavior_spatial_candidates::BehaviorSpatialCandidates;

pub(super) fn register_wander_execution(application: &mut App) {
    use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
        activated_behavior_actor, update_waiting_behavior_actors,
    };
    application.add_observer(
        activated_behavior_actor::<
            openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorWander,
        >
            .pipe(execute_current_wander_actions),
    );
    application.add_observer(
        activated_behavior_actor::<
            openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorEvasion,
        >
            .pipe(execute_current_evasion_actions),
    );

    use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorStepFixedSet;
    application.add_systems(
        FixedUpdate,
        (
            finish_wander_navigation_requests,
            update_waiting_behavior_actors
                .pipe(execute_current_wander_actions)
                .after(finish_wander_navigation_requests),
            update_waiting_behavior_actors.pipe(execute_current_evasion_actions),
        )
            .in_set(BehaviorStepFixedSet::Think)
            .run_if(in_state(GamePhase::InGame)),
    );
}

fn finish_wander_navigation_requests(
    mut arrivals: MessageReader<Arrived>,
    mut failures: MessageReader<NavigationFailed>,
    mut wanderers: Query<&mut BehaviorWanderNavigation>,
) {
    // A blocked direction ends this leg, allowing another direction next tick.
    for (actor, request_id) in failures
        .read()
        .map(|event| (event.entity, event.request_id))
        .chain(
            arrivals
                .read()
                .filter(|event| event.target.is_none())
                .map(|event| (event.entity, event.request_id)),
        )
    {
        if let Ok(mut wandering) = wanderers.get_mut(actor) {
            if wandering.request_id == Some(request_id) {
                wandering.request_id = None;
            }
        }
    }
}

fn execute_current_wander_actions(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    clocks: Query<&ZooClock>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    worlds: Query<(
        &HabitatIndex,
        &TopologyGrid,
        Option<&ActiveTerrainDerivedNavigationGraph>,
    )>,
    habitat_summaries: Query<&openzt2_zoo_state::habitat::habitat_types::HabitatSummary>,
    mut navigation_generations: Query<&mut NavigationRequestGeneration>,
    mut requests: MessageWriter<NavigateTo>,
    subject_types: super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    entities: Query<(Entity, &GlobalTransform, &DefinitionId, &WorldMember), UncratedWorldEntity>,
    routes: Query<&Route>,
    nearby: BehaviorSpatialCandidates,
    mut candidates: Local<Vec<Entity>>,
    mut animals: Query<
        (
            Entity,
            (&GlobalTransform, &WorldMember),
            (
                Option<&BehaviorTaskExecutionState>,
                Option<(&RunningBehaviorInstruction, &RunBehaviorWander)>,
            ),
            &mut BehaviorRandomStream,
            Option<&NavAgent>,
            Option<&LocomotionMode>,
            Option<&Destination>,
            Option<&mut BehaviorWanderNavigation>,
            Has<PendingBehaviorTaskFailure>,
            (Has<AnimalCrate>, Has<ScriptedCrating>),
            Has<Tranquilized>,
            Has<Dead>,
            Has<Staff>,
            Has<openzt2_zoo_state::animal_health::types::Escaped>,
            Option<&mut StaffSubmergedDepth>,
        ),
        (
            Or<(
                With<Animal>,
                With<Staff>,
                With<super::behavior_set_start_request_types::AmbientBehaviorSubject>,
            )>,
            Or<(With<RunBehaviorWander>, With<BehaviorWanderNavigation>)>,
        ),
    >,
) {
    for (
        actor,
        (transform, member),
        (task, running),
        mut random,
        agent,
        mode,
        destination,
        wandering,
        failing,
        crated,
        tranquilized,
        dead,
        staff,
        escaped,
        mut submerged_depth,
    ) in behavior_action_actors(&mut animals, activation.0)
    {
        let mut wandering = wandering;
        let immobilized = crated.0 || crated.1 || tranquilized || dead;
        if wandering.as_ref().is_some_and(|wandering| {
            failing
                || immobilized
                || task.zip(running).is_none_or(|(task, (instruction, _))| {
                    wandering.instruction != instruction.id() || !instruction.is_current(task)
                })
        }) {
            if wandering.as_ref().is_some_and(|wandering| {
                destination
                    .is_some_and(|destination| wandering.request_id == Some(destination.request_id))
            }) {
                commands.entity(actor).remove::<Destination>();
            }
            commands
                .entity(actor)
                .remove::<(BehaviorWanderNavigation, StaffSubmergedDepth)>();
            wandering = None;
            submerged_depth = None;
        }
        let Ok(clock) = clocks.get(member.root) else {
            continue;
        };
        let Some(task) =
            task.filter(|task| !failing && !immobilized && task.next_action_tick <= clock.tick)
        else {
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
        if action.maximum_duration.is_zero() {
            instruction.complete(actor, clock.tick, &mut commands);
            continue;
        }
        if wandering
            .as_ref()
            .is_some_and(|wandering| clock.tick >= wandering.until_tick)
        {
            if wandering.as_ref().is_some_and(|wandering| {
                destination
                    .is_some_and(|destination| wandering.request_id == Some(destination.request_id))
            }) {
                commands.entity(actor).remove::<Destination>();
            }
            commands
                .entity(actor)
                .remove::<(BehaviorWanderNavigation, StaffSubmergedDepth)>();
            instruction.complete(actor, clock.tick, &mut commands);
            continue;
        }
        let Some(definitions) = definitions.get(&definition_assets) else {
            continue;
        };
        // A wander that cannot move, at the `stop` speed or without a navigation
        // agent, still ends when its time runs out.
        let Ok((habitats, topology, graph)) = worlds.get(member.root) else {
            continue;
        };
        let (Some(agent), Some(graph)) = (
            agent.filter(|_| action.locomotion_speed != BehaviorGroundLocomotionSpeed::Stop),
            graph,
        ) else {
            if wandering.is_none() {
                let until_tick = draw_wander_until_tick(
                    action,
                    &mut random,
                    definitions
                        .simulation_timestep()
                        .expect("simulation timing is loaded"),
                    clock.tick,
                );
                commands.entity(actor).insert(BehaviorWanderNavigation {
                    instruction: instruction.id(),
                    until_tick,
                    request_id: None,
                });
            }
            continue;
        };
        // Staff walk without a locomotion mode of their own.
        if !(staff && mode.is_none())
            && mode != Some(&LocomotionMode::Ground)
            && !(mode == Some(&LocomotionMode::Swim)
                && agent.capabilities.contains(NavFlags::AMPHIBIOUS))
        {
            super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        let position = transform.translation();
        let mut avoids_entities = |start: Vec2, end: Vec2| {
            let Some(avoidance) = action.avoided_entities.as_ref() else {
                return true;
            };
            let margin = Vec2::splat(avoidance.radius_m.abs());
            nearby.in_bounds(
                member.root,
                start.min(end) - margin,
                start.max(end) + margin,
                &mut candidates,
            );
            let offset = end - start;
            let denominator = offset.length_squared().max(f32::EPSILON);
            let radius_squared = avoidance.radius_m.powi(2);
            entities
                .iter_many(candidates.iter().copied())
                .flatten()
                .all(|(entity, transform, definition, membership)| {
                    if entity == actor || membership.root != member.root {
                        return true;
                    }
                    let other = transform.translation().xz();
                    let along = ((other - start).dot(offset) / denominator).clamp(0.0, 1.0);
                    let closest = start + offset * along;
                    let initial_distance = start.distance_squared(other);
                    if closest.distance_squared(other) >= radius_squared
                        || (initial_distance < radius_squared
                            && along == 0.0
                            && end.distance_squared(other) > initial_distance)
                    {
                        return true;
                    }
                    let types = subject_types.borrow_for(entity);
                    let object = definitions.find_object(definition.0);
                    !avoidance.types.iter().any(|required| {
                        *required == definition.0
                            || types.contains(required)
                            || object.is_some_and(|object| {
                                object.behavior_type_identifiers.contains(required)
                            })
                    })
                })
        };
        if let Some(active) = wandering
            .as_deref_mut()
            .filter(|active| active.request_id.is_some())
        {
            let mut previous = position.xz();
            let safe = match routes.get(actor) {
                Err(_) => destination
                    .is_none_or(|destination| avoids_entities(previous, destination.world.xz())),
                Ok(route) => route.points.iter().skip(route.cursor).all(|point| {
                    let safe = avoids_entities(previous, point.xz());
                    previous = point.xz();
                    safe
                }),
            };
            if safe {
                continue;
            }
            if destination
                .is_some_and(|destination| active.request_id == Some(destination.request_id))
            {
                commands.entity(actor).remove::<Destination>();
            }
            active.request_id = None;
        }
        let habitat = locate_habitat_at_world_position(habitats, position.xz(), *topology);
        let navigation = graph.terrain_derived_navigation_graph();
        let tile_size_m = navigation.terrain_tile_size_centimetres() as f32 * 0.01;
        let mut selected = None;
        let mut count = 0_u32;
        // Include adjacent tiles so successive legs can leave the starting tile.
        for x in -1..=1 {
            for z in -1..=1 {
                let sample =
                    position + Vec3::new(x as f32 * tile_size_m, 0.0, z as f32 * tile_size_m);
                let sample_cm = (sample * 100.0).as_ivec3().to_array();
                for index in
                    navigation.find_candidate_node_indices_in_containing_terrain_tile(sample_cm)
                {
                    let Some(node) = navigation.find_navigation_node(index) else {
                        continue;
                    };
                    let walker = if staff {
                        NavFlags::STAFF
                    } else {
                        NavFlags::ANIMAL
                    };
                    if !node.eligibility_flags.contains(walker)
                        || node
                            .eligibility_flags
                            .contains(NavFlags::DISABLED_BY_DEFAULT)
                        || (action.avoid_water && node.eligibility_flags.contains(NavFlags::WATER))
                        || (action.avoid_land && !node.eligibility_flags.contains(NavFlags::WATER))
                        || (node.eligibility_flags.contains(NavFlags::WATER)
                            && !agent.capabilities.contains(NavFlags::AMPHIBIOUS))
                        || f32::from(node.clearance_centimetres) * 0.01 < agent.radius_m
                    {
                        continue;
                    }
                    let Some(point) =
                        navigation.calculate_navigation_node_world_position_centimetres(index)
                    else {
                        continue;
                    };
                    let point = Vec3::new(point[0] as f32, point[1] as f32, point[2] as f32) * 0.01;
                    if point.xz().distance_squared(position.xz()) <= agent.radius_m.max(0.1).powi(2)
                        || !avoids_entities(position.xz(), point.xz())
                        // An escaped animal roams wherever its breach leads.
                        || if escaped {
                            !super::escaped_animal_reaches(
                                habitats,
                                *topology,
                                &habitat_summaries,
                                point.xz(),
                            )
                        } else {
                            locate_habitat_at_world_position(habitats, point.xz(), *topology)
                                != habitat
                        }
                    {
                        continue;
                    }
                    count = count.saturating_add(1);
                    if random.range_u32(count) == Some(0) {
                        selected = Some(point);
                    }
                }
            }
        }
        let Some(destination) = selected else {
            super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let Ok(mut generation) = navigation_generations.get_mut(actor) else {
            continue;
        };
        let request_id = generation.advance();
        requests.write(NavigateTo {
            entity: actor,
            request_id,
            destination,
            arrival_radius_m: agent.radius_m.max(0.1),
        });
        // A swimming employee keeps each leg within the authored depth band.
        if staff && mode == Some(&LocomotionMode::Swim) && action.maximum_depth_m > 0.0 {
            let depth = action.minimum_depth_m
                + (action.maximum_depth_m - action.minimum_depth_m) * random.0.unit_f32();
            if let Some(mut submerged_depth) = submerged_depth {
                submerged_depth.set_if_neq(StaffSubmergedDepth(depth));
            } else {
                commands.entity(actor).insert(StaffSubmergedDepth(depth));
            }
        }
        if let Some(wandering) = wandering.as_deref_mut() {
            wandering.request_id = Some(request_id);
        } else {
            let until_tick = draw_wander_until_tick(
                action,
                &mut random,
                definitions
                    .simulation_timestep()
                    .expect("simulation timing is loaded"),
                clock.tick,
            );
            commands.entity(actor).insert(BehaviorWanderNavigation {
                instruction: instruction.id(),
                until_tick,
                request_id: Some(request_id),
            });
        }
    }
}

fn draw_wander_until_tick(
    action: &BehaviorWanderAction,
    random: &mut BehaviorRandomStream,
    step: std::time::Duration,
    now: u64,
) -> u64 {
    let duration = action.minimum_duration
        + (action.maximum_duration - action.minimum_duration)
            .mul_f64(f64::from(random.0.unit_f32()));
    let ticks = duration.as_nanos().div_ceil(step.as_nanos());
    now.saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX))
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn execute_current_evasion_actions(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    clocks: Query<&ZooClock>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut navigation_generations: Query<&mut NavigationRequestGeneration>,
    subject_types: super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    entities: Query<(Entity, &GlobalTransform, &DefinitionId, &WorldMember), UncratedWorldEntity>,
    nearby: BehaviorSpatialCandidates,
    mut candidates: Local<Vec<Entity>>,
    mut actors: Query<
        (
            Entity,
            &GlobalTransform,
            &WorldMember,
            Option<&BehaviorTaskExecutionState>,
            Option<(&RunningBehaviorInstruction, &RunBehaviorEvasion)>,
            Option<&mut Destination>,
            Option<&BehaviorEvasionNavigation>,
            Option<&mut BehaviorEvasionSteering>,
            &mut BehaviorRandomStream,
            Has<PendingBehaviorTaskFailure>,
            (Has<AnimalCrate>, Has<ScriptedCrating>),
            Has<Tranquilized>,
            Has<Dead>,
        ),
        (
            Or<(
                With<Animal>,
                With<Staff>,
                With<super::behavior_set_start_request_types::AmbientBehaviorSubject>,
            )>,
            Or<(
                With<RunBehaviorEvasion>,
                With<BehaviorEvasionNavigation>,
                With<BehaviorEvasionSteering>,
            )>,
        ),
    >,
) {
    let definitions = definitions.get(&definition_assets);
    for (
        actor,
        transform,
        member,
        task,
        running,
        mut destination,
        evading,
        mut steering,
        mut random,
        failing,
        crated,
        tranquilized,
        dead,
    ) in behavior_action_actors(&mut actors, activation.0)
    {
        let immobilized = failing || crated.0 || crated.1 || tranquilized || dead;
        let current = evading.filter(|evading| {
            !immobilized
                && task.zip(running).is_some_and(|(task, (instruction, _))| {
                    evading.instruction == instruction.id() && instruction.is_current(task)
                })
        });
        if evading.is_some() && current.is_none() {
            if evading.is_some_and(|evading| {
                destination
                    .as_ref()
                    .is_some_and(|destination| destination.request_id == evading.request_id)
            }) {
                commands.entity(actor).remove::<Destination>();
                destination = None;
            }
            commands
                .entity(actor)
                .remove::<(BehaviorEvasionNavigation, BehaviorEvasionSteering)>();
            steering = None;
        }
        let Ok(clock) = clocks.get(member.root) else {
            continue;
        };
        let Some(task) = task.filter(|task| !immobilized && task.next_action_tick <= clock.tick)
        else {
            continue;
        };
        let Some(definitions) = definitions else {
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
        let position = transform.translation();
        let mut target_direction = Vec2::ZERO;
        let mut target_near = false;
        if let Some((_, target, _, _)) = task
            .target()
            .filter(|target| *target != actor)
            .and_then(|target| entities.get(target).ok())
            .filter(|(_, _, _, target)| target.root == member.root)
        {
            let away = position.xz() - target.translation().xz();
            target_near = away.length_squared() <= action.radius_m.powi(2);
            target_direction = away.normalize_or_zero();
            let forward = transform
                .affine()
                .transform_vector3(Vec3::Z)
                .xz()
                .normalize_or_zero();
            if target_direction.dot(forward) >= 0.0 {
                let right = Vec2::new(forward.y, -forward.x);
                target_direction =
                    (forward + right * (random.0.unit_f32() * 2.0 - 1.0)).normalize_or_zero();
            }
        }
        let mut entity_direction = Vec2::ZERO;
        let mut entity_near = false;
        let margin = Vec2::splat(action.radius_m.abs());
        nearby.in_bounds(
            member.root,
            position.xz() - margin,
            position.xz() + margin,
            &mut candidates,
        );
        nearby.preserve_query_order(&mut candidates);
        for (entity, other, definition, other_world) in
            entities.iter_many(candidates.iter().copied()).flatten()
        {
            if entity == actor || other_world.root != member.root {
                continue;
            }
            let away = position.xz() - other.translation().xz();
            if away.length_squared() > action.radius_m.powi(2) {
                continue;
            }
            let types = subject_types.borrow_for(entity);
            if action.avoided_types.iter().any(|required| {
                *required == definition.0
                    || types.contains(required)
                    || definitions
                        .find_object(definition.0)
                        .is_some_and(|object| object.behavior_type_identifiers.contains(required))
            }) {
                entity_direction += away;
                entity_near = true;
            }
        }
        let timed_out = current.map_or(action.maximum_duration.is_zero(), |evading| {
            clock.tick >= evading.until_tick
        });
        let clear = !target_near && !entity_near;
        if clear || timed_out {
            if current.is_some_and(|evading| {
                destination
                    .as_ref()
                    .is_some_and(|destination| destination.request_id == evading.request_id)
            }) {
                commands.entity(actor).remove::<Destination>();
            }
            commands
                .entity(actor)
                .remove::<(BehaviorEvasionNavigation, BehaviorEvasionSteering)>();
            if timed_out && action.fail_on_timeout {
                super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            } else {
                instruction.complete(actor, clock.tick, &mut commands);
            }
            continue;
        }
        let Ok(mut generation) = navigation_generations.get_mut(actor) else {
            continue;
        };
        let request_id = current.map_or_else(|| generation.advance(), |current| current.request_id);
        let mut direction = target_direction + entity_direction.normalize_or_zero();
        if direction.length_squared() <= f32::EPSILON {
            direction = Vec2::new(random.0.unit_f32() * 2.0 - 1.0, -1.0);
        }
        let steering_value = BehaviorEvasionSteering {
            request_id,
            direction,
            obstacle_radius_m: action.obstacle_radius_m,
            avoid_water: action.avoid_water,
            avoid_land: action.avoid_land,
        };
        if let Some(mut steering) = steering {
            steering.set_if_neq(steering_value);
        } else {
            commands.entity(actor).insert(steering_value);
        }
        if current.is_none()
            || destination
                .as_ref()
                .is_none_or(|destination| destination.request_id != request_id)
        {
            let destination_value = Destination {
                request_id,
                world: position + Vec3::new(direction.x, 0.0, direction.y).normalize_or_zero(),
                arrival_radius_m: 0.0,
            };
            if let Some(mut destination) = destination {
                destination.set_if_neq(destination_value);
            } else {
                commands.entity(actor).insert(destination_value);
            }
        }
        if current.is_none() {
            let ticks = action.maximum_duration.as_nanos().div_ceil(
                definitions
                    .simulation_timestep()
                    .expect("simulation timing is loaded")
                    .as_nanos(),
            );
            commands.entity(actor).insert(BehaviorEvasionNavigation {
                instruction: instruction.id(),
                until_tick: clock
                    .tick
                    .saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX)),
                request_id,
            });
        }
    }
}
