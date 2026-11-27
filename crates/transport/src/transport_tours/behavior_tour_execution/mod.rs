use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_assets::behavior::behavior_asset_types::{
    BehaviorDocumentAsset, LoadedBehaviorDocumentCollection,
};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_world_state::{
    application_schedule::FixedGameSet, simulation_time::simulation_clock_types::ZooClock,
    world_spawn::world_membership_types::WorldMember,
};
use openzt2_zoo_state::{
    behavior_task_execution_types::{
        running_actions::{
            mark_behavior_task_for_failure_and_stop_navigation, AuthoredTourRide,
            BehaviorTaskReplaced, GuestVisitedTourStations, RunBehaviorTour,
            RunningBehaviorInstruction, SelectedBehaviorSet, WhenBehaviorAction,
        },
        BehaviorTaskExecutionState, BehaviorTaskReturnFrame,
        MAXIMUM_BEHAVIOR_TASK_RETURN_FRAME_COUNT,
    },
    guests::guest_simulation_types::GuestArchetype,
    locomotion::locomotion_types::{
        Arrived, DockAt, NavAgent, NavigationFailed, NavigationRequestGeneration,
    },
    transport_tours::{
        tour_scoring_types::TransportTripCompleted,
        transport_circuit_types::{CircuitMember, CircuitMembers, TransportCircuit},
        transport_rider_types::{BoardTransportRequest, TransportRider, WaitingForTransport},
        transport_topology_types::TransportStation,
    },
};

#[derive(Component, Copy, Clone)]
enum TourProgress {
    Approach(u64),
    BoardRequested(u64),
    Queued,
    Aboard,
    Disembarked,
}

#[derive(Component)]
struct SuspendedTourInstruction {
    execution: u64,
    depth: usize,
    frame: BehaviorTaskReturnFrame,
    target: Option<Entity>,
}

pub(super) fn register(app: &mut App) {
    app.add_observer(begin_tour)
        .add_observer(clear_finished_tour_instruction)
        .add_observer(clear_replaced_tour)
        .add_observer(clear_abandoned_tour)
        .add_systems(
            FixedUpdate,
            advance_tours
                .in_set(FixedGameSet::Act)
                .before(super::transport_rider_boarding::accept_transport_queue_reservations)
                .run_if(in_state(
                    openzt2_game_assets::application_lifecycle::GamePhase::InGame,
                )),
        )
        .add_systems(
            FixedUpdate,
            start_seated_tour_feedback
                .after(advance_tours)
                .after(super::tour_view_scoring::observe_visible_tour_subjects_and_accumulate_rider_scores)
                .in_set(FixedGameSet::Act)
                .run_if(in_state(openzt2_game_assets::application_lifecycle::GamePhase::InGame)),
        );
}

fn begin_tour(
    activation: On<WhenBehaviorAction<RunBehaviorTour>>,
    mut guests: Query<(
        &RunningBehaviorInstruction,
        &mut BehaviorTaskExecutionState,
        &WorldMember,
        &Transform,
        &NavAgent,
        &mut NavigationRequestGeneration,
        Option<&SuspendedTourInstruction>,
        Option<&TourProgress>,
    )>,
    stations: Query<(
        &TransportStation,
        &WorldMember,
        &CircuitMember,
        &GlobalTransform,
    )>,
    circuits: Query<(&TransportCircuit, &CircuitMembers)>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    clocks: Query<&ZooClock>,
    mut docks: MessageWriter<DockAt>,
    mut commands: Commands,
) {
    let actor = activation.entity;
    let Ok((instruction, mut task, member, pose, agent, mut requests, suspended, progress)) =
        guests.get_mut(actor)
    else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    if !instruction.is_current(&task) {
        return;
    }
    if let Some(suspended) = suspended.filter(|suspended| {
        suspended.execution == task.execution_id
            && task.is_at_exact_frame(&suspended.frame, suspended.depth)
    }) {
        task.target = suspended.target;
        commands.entity(actor).remove::<SuspendedTourInstruction>();
        if matches!(progress, Some(TourProgress::Disembarked)) {
            if let Ok(clock) = clocks.get(member.root) {
                instruction.complete(actor, clock.tick, &mut commands);
            }
        }
        return;
    }
    let endpoints = (|| {
        let source = task.target?;
        let (station, world, circuit, global) = stations.get(source).ok()?;
        let (route, members) = circuits.get(circuit.0).ok()?;
        if route.closed || world.root != member.root {
            return None;
        }
        let is_station = |entity: Entity| {
            stations
                .get(entity)
                .is_ok_and(|(_, world, other, _)| world.root == member.root && other.0 == circuit.0)
        };
        let destination = task
            .object
            .filter(|entity| is_station(*entity))
            .or_else(|| {
                members
                    .iter()
                    .find(|entity| *entity != source && is_station(*entity))
            })
            .unwrap_or(source);
        let view = definitions.get(&assets)?;
        let station = view.find_station(station.definition)?;
        let local = station
            .passenger_entry_offsets_cm
            .iter()
            .map(|&[x, z]| Vec3::new(f32::from(x), 0.0, f32::from(z)) * 0.01)
            .min_by(|left, right| {
                global
                    .transform_point(*left)
                    .distance_squared(pose.translation)
                    .total_cmp(
                        &global
                            .transform_point(*right)
                            .distance_squared(pose.translation),
                    )
            })?;
        Some((source, destination, local))
    })();
    let Some((source, destination, local)) = endpoints else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    let request_id = requests.advance();
    commands.entity(actor).insert((
        AuthoredTourRide {
            source,
            destination,
        },
        TourProgress::Approach(request_id),
    ));
    docks.write(DockAt {
        entity: actor,
        target: source,
        request_id,
        local_point: local,
        local_forward: Vec3::Z,
        radius_m: agent.radius_m.max(0.05),
    });
}

fn advance_tours(
    mut arrivals: MessageReader<Arrived>,
    mut failures: MessageReader<NavigationFailed>,
    mut trips: MessageReader<TransportTripCompleted>,
    clocks: Query<&ZooClock>,
    stations: Query<(), With<TransportStation>>,
    mut guests: Query<(
        Entity,
        Option<&RunningBehaviorInstruction>,
        &BehaviorTaskExecutionState,
        &WorldMember,
        &AuthoredTourRide,
        &mut TourProgress,
        Has<WaitingForTransport>,
        Has<TransportRider>,
        Has<RunBehaviorTour>,
    )>,
    mut visited_stations: Query<&mut GuestVisitedTourStations>,
    mut boarding: MessageWriter<BoardTransportRequest>,
    mut commands: Commands,
) {
    for trip in trips.read() {
        let Ok((actor, step, task, world, ride, mut progress, _, aboard, in_ride)) =
            guests.get_mut(trip.guest)
        else {
            continue;
        };
        if trip.station == ride.destination && (aboard || matches!(*progress, TourProgress::Aboard))
        {
            if let Ok(clock) = clocks.get(world.root) {
                if let Ok(mut visited) = visited_stations.get_mut(actor) {
                    for station in [ride.source, ride.destination] {
                        if !visited.0.contains(&station) {
                            visited.0.insert(station);
                        }
                    }
                }
                if let Some(step) = step.filter(|step| in_ride && step.is_current(task)) {
                    step.complete(actor, clock.tick, &mut commands);
                } else {
                    *progress = TourProgress::Disembarked;
                }
            }
        }
    }
    for failure in failures.read() {
        let Ok((actor, Some(step), task, _, _, progress, _, _, true)) = guests.get(failure.entity)
        else {
            continue;
        };
        if step.is_current(task)
            && matches!(progress, TourProgress::Approach(id) if *id == failure.request_id)
        {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
    }
    for (actor, step, task, world, ride, mut progress, queued, aboard, in_ride) in &mut guests {
        let Some(step) = step.filter(|_| in_ride) else {
            continue;
        };
        if !step.is_current(task) {
            continue;
        }
        let Ok(clock) = clocks.get(world.root) else {
            continue;
        };
        if !stations.contains(ride.source) || !stations.contains(ride.destination) {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        match *progress {
            TourProgress::BoardRequested(tick) if clock.tick > tick => {
                if aboard {
                    *progress = TourProgress::Aboard;
                } else if queued {
                    *progress = TourProgress::Queued;
                } else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                }
            }
            TourProgress::Queued if aboard => *progress = TourProgress::Aboard,
            TourProgress::Queued if !queued => {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands)
            }
            _ => {}
        }
    }
    for arrival in arrivals.read() {
        let Ok((actor, Some(step), task, world, ride, mut progress, _, _, true)) =
            guests.get_mut(arrival.entity)
        else {
            continue;
        };
        if !step.is_current(task)
            || arrival.target != Some(ride.source)
            || !matches!(*progress, TourProgress::Approach(id) if id == arrival.request_id)
        {
            continue;
        }
        if let Ok(clock) = clocks.get(world.root) {
            boarding.write(BoardTransportRequest {
                guest: actor,
                station: ride.source,
            });
            *progress = TourProgress::BoardRequested(clock.tick);
        }
    }
}

fn start_seated_tour_feedback(
    mut requests: MessageReader<super::tour_view_scoring::TourSeatFeedbackRequested>,
    guests: Query<
        (
            &WorldMember,
            &GuestArchetype,
            &RunningBehaviorInstruction,
            &BehaviorTaskExecutionState,
            &TourProgress,
        ),
        (
            With<RunBehaviorTour>,
            With<TransportRider>,
            Without<SuspendedTourInstruction>,
        ),
    >,
    subjects: Query<&WorldMember>,
    clocks: Query<&ZooClock>,
    definitions: Res<WorldDefinitions>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    documents: Res<LoadedBehaviorDocumentCollection>,
    behavior_assets: Res<Assets<BehaviorDocumentAsset>>,
    mut commands: Commands,
) {
    let (Some(definitions), Some(declarations)) = (
        definitions.get(&world_assets),
        documents.create_declaration_index_view(&behavior_assets),
    ) else {
        requests.clear();
        return;
    };
    let mut started = bevy::ecs::entity::EntityHashSet::default();
    for request in requests.read() {
        let Ok((world, archetype, step, task, progress)) = guests.get(request.rider) else {
            continue;
        };
        if matches!(progress, TourProgress::Disembarked)
            || !step.is_current(task)
            || task.depth >= MAXIMUM_BEHAVIOR_TASK_RETURN_FRAME_COUNT
            || !subjects
                .get(request.subject)
                .is_ok_and(|subject| subject.root == world.root)
        {
            continue;
        }
        let Ok(clock) = clocks.get(world.root) else {
            continue;
        };
        let Some(guest) = definitions.find_guest(archetype.0) else {
            continue;
        };
        let Some((document, declaration)) = declarations.find_behavior_set_location(
            request.program,
            std::iter::once(guest.id)
                .chain(guest.behavior_subject_type_identifiers.iter().copied()),
        ) else {
            continue;
        };
        if !started.insert(request.rider) {
            continue;
        }
        commands
            .entity(request.rider)
            .insert(SuspendedTourInstruction {
                execution: task.execution_id,
                depth: task.depth,
                frame: task.create_return_frame_after_action(task.action),
                target: task.target,
            });
        step.enter_set(
            request.rider,
            SelectedBehaviorSet {
                program: request.program,
                document,
                declaration,
                tick: clock.tick,
                repeat: Some(task.repetitions),
                target: Some(Some(request.subject)),
            },
            &mut commands,
        );
    }
}

fn clear_finished_tour_instruction(
    removed: On<Remove<RunBehaviorTour>>,
    suspended: Query<(&SuspendedTourInstruction, &BehaviorTaskExecutionState)>,
    mut commands: Commands,
) {
    if suspended.get(removed.entity).is_ok_and(|(origin, task)| {
        origin.execution == task.execution_id && task.depth > origin.depth
    }) {
        return;
    }
    clear_tour(removed.entity, &mut commands);
}

fn clear_replaced_tour(replaced: On<BehaviorTaskReplaced>, mut commands: Commands) {
    clear_tour(replaced.actor, &mut commands);
}

fn clear_abandoned_tour(removed: On<Remove<BehaviorTaskExecutionState>>, mut commands: Commands) {
    clear_tour(removed.entity, &mut commands);
}

fn clear_tour(actor: Entity, commands: &mut Commands) {
    commands.entity(actor).try_remove::<(
        AuthoredTourRide,
        TourProgress,
        SuspendedTourInstruction,
        WaitingForTransport,
    )>();
}
