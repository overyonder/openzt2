use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
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
        BehaviorTaskExecutionState,
        running_actions::{
            AuthoredTourRide, RunBehaviorTour, RunningBehaviorInstruction, WhenBehaviorAction,
            mark_behavior_task_for_failure_and_stop_navigation,
        },
    },
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
}

pub(super) fn register(app: &mut App) {
    app.add_observer(begin_tour)
        .add_observer(clear_tour)
        .add_systems(
            FixedUpdate,
            advance_tours
                .in_set(FixedGameSet::Act)
                .before(super::transport_rider_boarding::accept_transport_queue_reservations)
                .run_if(in_state(
                    openzt2_game_assets::application_lifecycle::GamePhase::InGame,
                )),
        );
}

fn begin_tour(
    activation: On<WhenBehaviorAction<RunBehaviorTour>>,
    mut guests: Query<(
        &RunningBehaviorInstruction,
        &BehaviorTaskExecutionState,
        &WorldMember,
        &Transform,
        &NavAgent,
        &mut NavigationRequestGeneration,
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
    mut docks: MessageWriter<DockAt>,
    mut commands: Commands,
) {
    let actor = activation.entity;
    let Ok((instruction, task, member, pose, agent, mut requests)) = guests.get_mut(actor) else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    if !instruction.is_current(task) {
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
    mut guests: Query<
        (
            Entity,
            &RunningBehaviorInstruction,
            &BehaviorTaskExecutionState,
            &WorldMember,
            &AuthoredTourRide,
            &mut TourProgress,
            Has<WaitingForTransport>,
            Has<TransportRider>,
        ),
        With<RunBehaviorTour>,
    >,
    mut boarding: MessageWriter<BoardTransportRequest>,
    mut commands: Commands,
) {
    for trip in trips.read() {
        let Ok((actor, step, task, world, ride, progress, _, aboard)) = guests.get(trip.guest)
        else {
            continue;
        };
        if trip.station == ride.destination
            && (aboard || matches!(progress, TourProgress::Aboard))
            && step.is_current(task)
        {
            if let Ok(clock) = clocks.get(world.root) {
                step.complete(actor, clock.tick, &mut commands);
            }
        }
    }
    for failure in failures.read() {
        let Ok((actor, step, task, _, _, progress, _, _)) = guests.get(failure.entity) else {
            continue;
        };
        if step.is_current(task)
            && matches!(progress, TourProgress::Approach(id) if *id == failure.request_id)
        {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
    }
    for (actor, step, task, world, ride, mut progress, queued, aboard) in &mut guests {
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
        let Ok((actor, step, task, world, ride, mut progress, _, _)) =
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

fn clear_tour(removed: On<Remove<RunBehaviorTour>>, mut commands: Commands) {
    commands
        .entity(removed.entity)
        .try_remove::<(AuthoredTourRide, TourProgress, WaitingForTransport)>();
}
