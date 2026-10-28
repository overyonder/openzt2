use bevy::prelude::*;
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    behavior::behavior_asset_types::BehaviorDocumentAsset,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::behavior::action_record::BehaviorAction;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use openzt2_world_state::{
    application_schedule::FixedGameSet, simulation_time::simulation_clock_types::ZooClock,
};
use openzt2_zoo_state::animal_health::types::{Dead, Tranquilized};
use openzt2_zoo_state::{
    animal_behavior::behavior_random_stream_state::BehaviorRandomStream,
    animal_lifecycle::types::Animal,
    behavior_task_execution_types::{
        advance_behavior_task_to_next_action, find_current_behavior_task_action,
        BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    habitat::{
        habitat_membership_and_containment::locate_habitat_at_world_position,
        habitat_types::HabitatIndex,
    },
    locomotion::locomotion_types::{
        ActiveTerrainDerivedNavigationGraph, Arrived, Destination, LocomotionMode, NavAgent,
        NavFlags, NavigateTo, NavigationFailed, NavigationRequestSequence, Route,
    },
    staff::staff_employment_types::{Staff, StaffSubmergedDepth},
    topology::topology_graph_types::TopologyGrid,
};

use openzt2_zoo_state::behavior_task_execution_types::{
    BehaviorEvasionNavigation, BehaviorWanderNavigation,
};

pub(super) fn register_wander_execution(application: &mut App) {
    application.add_systems(
        FixedUpdate,
        (
            finish_wander_navigation_requests,
            execute_current_wander_actions,
            execute_current_evasion_actions,
        )
            .chain()
            .after(super::behavior_task_phase_transition_execution::advance_behavior_tasks_through_return_failure_completion_and_final_outcome)
            .before(openzt2_zoo_state::locomotion::AnimalAnimationNavigationProjection)
            .in_set(FixedGameSet::Think)
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
    mut commands: Commands,
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    graph: Option<Res<ActiveTerrainDerivedNavigationGraph>>,
    habitats: Res<HabitatIndex>,
    topology: Res<TopologyGrid>,
    mut sequence: ResMut<NavigationRequestSequence>,
    mut requests: MessageWriter<NavigateTo>,
    subject_types: super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    entities: Query<
        (Entity, &GlobalTransform, &DefinitionId, &WorldMember),
        Without<WorldEntityIsCrated>,
    >,
    routes: Query<&Route>,
    mut avoided_positions: Local<Vec<Vec2>>,
    mut animals: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&mut BehaviorTaskExecutionState>,
            &mut BehaviorRandomStream,
            Option<&NavAgent>,
            Option<&LocomotionMode>,
            Option<&Destination>,
            Option<&mut BehaviorWanderNavigation>,
            Has<PendingBehaviorTaskFailure>,
            Has<WorldEntityIsCrated>,
            Has<Tranquilized>,
            Has<Dead>,
            Has<Staff>,
        ),
        Or<(With<Animal>, With<Staff>)>,
    >,
) {
    for (
        actor,
        transform,
        task,
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
    ) in &mut animals
    {
        let mut task = task;
        let mut wandering = wandering;
        let immobilized = crated || tranquilized || dead;
        if wandering.as_ref().is_some_and(|wandering| {
            failing || immobilized || task.as_ref().is_none_or(|task| !wandering.owns(task))
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
        }
        let Some(task) = task
            .as_deref_mut()
            .filter(|task| !failing && !immobilized && task.next_action_tick <= clock.tick)
        else {
            continue;
        };
        let Some(BehaviorAction::Wander(action)) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, task))
        else {
            continue;
        };
        if action.maximum_duration.is_zero() {
            advance_behavior_task_to_next_action(task, clock.tick);
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
            advance_behavior_task_to_next_action(task, clock.tick);
            continue;
        }
        let (Some(agent), Some(graph), Some(definitions)) =
            (agent, graph.as_deref(), definitions.get(&definition_assets))
        else {
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
        avoided_positions.clear();
        if let Some(avoidance) = &action.avoided_entities {
            if let Ok((_, _, _, world)) = entities.get(actor) {
                for (entity, other_transform, definition, other_world) in &entities {
                    if entity == actor || other_world.root != world.root {
                        continue;
                    }
                    let types = subject_types.borrow_for(entity);
                    let object = definitions.find_object(definition.0);
                    if avoidance.types.iter().any(|required| {
                        *required == definition.0
                            || types.contains(required)
                            || object.is_some_and(|object| {
                                object.behavior_type_identifiers.contains(required)
                            })
                    }) {
                        avoided_positions.push(other_transform.translation().xz());
                    }
                }
            }
        }
        let avoids_entities = |start: Vec2, end: Vec2| {
            action.avoided_entities.as_ref().is_none_or(|avoidance| {
                avoided_positions.iter().all(|other| {
                    let offset = end - start;
                    let along = ((*other - start).dot(offset)
                        / offset.length_squared().max(f32::EPSILON))
                    .clamp(0.0, 1.0);
                    let closest = start + offset * along;
                    let initial_distance = start.distance_squared(*other);
                    let radius_squared = avoidance.radius_m.powi(2);
                    // An actor already inside the radius may move directly away.
                    closest.distance_squared(*other) >= radius_squared
                        || (initial_distance < radius_squared
                            && along == 0.0
                            && end.distance_squared(*other) > initial_distance)
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
        let habitat = locate_habitat_at_world_position(&habitats, position.xz(), *topology);
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
                        || locate_habitat_at_world_position(&habitats, point.xz(), *topology)
                            != habitat
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
        let request_id = sequence.next();
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
            commands.entity(actor).insert(StaffSubmergedDepth(depth));
        }
        if let Some(wandering) = wandering.as_deref_mut() {
            wandering.request_id = Some(request_id);
        } else {
            let duration = action.minimum_duration
                + (action.maximum_duration - action.minimum_duration)
                    .mul_f64(f64::from(random.0.unit_f32()));
            let ticks = duration
                .as_nanos()
                .saturating_mul(u128::from(definitions.timing().fixed_hz))
                .div_ceil(1_000_000_000);
            commands.entity(actor).insert(BehaviorWanderNavigation {
                execution_id: task.execution_id,
                origin: task.create_return_frame_after_action(task.action),
                depth: task.stack.len(),
                until_tick: clock
                    .tick
                    .saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX)),
                request_id: Some(request_id),
            });
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn execute_current_evasion_actions(
    mut commands: Commands,
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut sequence: ResMut<NavigationRequestSequence>,
    subject_types: super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    entities: Query<
        (Entity, &GlobalTransform, &DefinitionId, &WorldMember),
        Without<WorldEntityIsCrated>,
    >,
    mut actors: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&mut BehaviorTaskExecutionState>,
            Option<&Destination>,
            Option<&BehaviorEvasionNavigation>,
            &mut BehaviorRandomStream,
            Has<PendingBehaviorTaskFailure>,
            Has<WorldEntityIsCrated>,
            Has<Tranquilized>,
            Has<Dead>,
        ),
        Or<(With<Animal>, With<Staff>)>,
    >,
) {
    use openzt2_zoo_state::behavior_task_execution_types::BehaviorEvasionSteering;
    let definitions = definitions.get(&definition_assets);
    for (
        actor,
        transform,
        task,
        destination,
        evading,
        mut random,
        failing,
        crated,
        tranquilized,
        dead,
    ) in &mut actors
    {
        let immobilized = failing || crated || tranquilized || dead;
        let current = evading
            .filter(|evading| !immobilized && task.as_ref().is_some_and(|task| evading.owns(task)));
        if evading.is_some() && current.is_none() {
            if evading.is_some_and(|evading| {
                destination.is_some_and(|destination| destination.request_id == evading.request_id)
            }) {
                commands.entity(actor).remove::<Destination>();
            }
            commands
                .entity(actor)
                .remove::<(BehaviorEvasionNavigation, BehaviorEvasionSteering)>();
        }
        let Some(mut task) =
            task.filter(|task| !immobilized && task.next_action_tick <= clock.tick)
        else {
            continue;
        };
        let Some(definitions) = definitions else {
            continue;
        };
        let Some(BehaviorAction::Evasion(action)) = documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        let position = transform.translation();
        let mut target_direction = Vec2::ZERO;
        let mut target_near = false;
        if let Some((_, target, _, _)) = task
            .target()
            .filter(|target| *target != actor)
            .and_then(|target| entities.get(target).ok())
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
        if let Ok((_, _, _, world)) = entities.get(actor) {
            for (entity, other, definition, other_world) in &entities {
                if entity == actor || other_world.root != world.root {
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
                        || definitions.find_object(definition.0).is_some_and(|object| {
                            object.behavior_type_identifiers.contains(required)
                        })
                }) {
                    entity_direction += away;
                    entity_near = true;
                }
            }
        }
        let timed_out = current.map_or(action.maximum_duration.is_zero(), |evading| {
            clock.tick >= evading.until_tick
        });
        let clear = !target_near && !entity_near;
        if clear || timed_out {
            if current.is_some_and(|evading| {
                destination.is_some_and(|destination| destination.request_id == evading.request_id)
            }) {
                commands.entity(actor).remove::<Destination>();
            }
            commands
                .entity(actor)
                .remove::<(BehaviorEvasionNavigation, BehaviorEvasionSteering)>();
            if timed_out && action.fail_on_timeout {
                super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            } else {
                advance_behavior_task_to_next_action(&mut task, clock.tick);
            }
            continue;
        }
        let request_id = current.map_or_else(|| sequence.next(), |current| current.request_id);
        let mut direction = target_direction + entity_direction.normalize_or_zero();
        if direction.length_squared() <= f32::EPSILON {
            direction = Vec2::new(random.0.unit_f32() * 2.0 - 1.0, -1.0);
        }
        commands.entity(actor).insert(BehaviorEvasionSteering {
            request_id,
            direction,
            obstacle_radius_m: action.obstacle_radius_m,
            avoid_water: action.avoid_water,
            avoid_land: action.avoid_land,
        });
        if destination.is_none_or(|destination| destination.request_id != request_id) {
            commands.entity(actor).insert(Destination {
                request_id,
                world: position + Vec3::new(direction.x, 0.0, direction.y).normalize_or_zero(),
                arrival_radius_m: 0.0,
            });
        }
        if current.is_none() {
            let ticks = action
                .maximum_duration
                .as_nanos()
                .saturating_mul(u128::from(definitions.timing().fixed_hz))
                .div_ceil(1_000_000_000);
            commands.entity(actor).insert((
                BehaviorEvasionNavigation {
                    execution_id: task.execution_id,
                    origin: task.create_return_frame_after_action(task.action),
                    depth: task.stack.len(),
                    until_tick: clock
                        .tick
                        .saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX)),
                    request_id,
                },
                Destination {
                    request_id,
                    world: position + Vec3::new(direction.x, 0.0, direction.y).normalize_or_zero(),
                    arrival_radius_m: 0.0,
                },
            ));
        }
    }
}
