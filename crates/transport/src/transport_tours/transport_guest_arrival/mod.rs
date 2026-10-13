use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestVisitPurpose;

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::economy::service_types::ServiceReservation;
use openzt2_zoo_state::guests::guest_simulation_types::Guest;
use openzt2_zoo_state::guests::guest_simulation_types::GuestDestination;
use openzt2_zoo_state::guests::guest_simulation_types::GuestNavigationRequest;
use openzt2_zoo_state::guests::guest_simulation_types::GuestPhase;
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;

use openzt2_zoo_state::transport_tours::transport_rider_types::BoardTransportRequest;
use openzt2_zoo_state::transport_tours::transport_rider_types::TransportRider;
use openzt2_zoo_state::transport_tours::transport_rider_types::WaitingForTransport;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation;

#[allow(
    clippy::type_complexity,
    reason = "guest ownership is expressed by ECS query filters"
)]
pub(super) fn request_transport_boarding_for_owned_guest_arrivals(
    mut arrivals: MessageReader<Arrived>,
    mut failures: MessageReader<NavigationFailed>,
    mut guests: Query<
        (
            &GuestDestination,
            &mut GuestNavigationRequest,
            &GuestPhase,
            &WorldMember,
        ),
        (
            With<Guest>,
            Without<WaitingForTransport>,
            Without<TransportRider>,
            Without<ServiceReservation>,
            Without<BehaviorTaskExecutionState>,
            Without<PendingBehaviorSet>,
        ),
    >,
    stations: Query<(&Visibility, &WorldMember), With<TransportStation>>,
    mut boarding: MessageWriter<BoardTransportRequest>,
    mut commands: Commands,
) {
    let failed_requests: bevy::platform::collections::HashSet<_> = failures
        .read()
        .map(|failure| (failure.entity, failure.request_id))
        .collect();
    for arrival in arrivals.read() {
        let Ok((destination, mut navigation, phase, world)) = guests.get_mut(arrival.entity) else {
            continue;
        };
        if destination.purpose != GuestVisitPurpose::Tour
            || *phase != GuestPhase::Visiting
            || navigation.0 != Some(arrival.request_id)
            || arrival
                .target
                .is_some_and(|target| target != destination.entity)
            || failed_requests.contains(&(arrival.entity, arrival.request_id))
        {
            continue;
        }
        navigation.0 = None;
        commands.entity(arrival.entity).remove::<GuestDestination>();
        if stations
            .get(destination.entity)
            .is_ok_and(|(visibility, station_world)| {
                *visibility != Visibility::Hidden && station_world.root == world.root
            })
        {
            boarding.write(BoardTransportRequest {
                guest: arrival.entity,
                station: destination.entity,
            });
        }
    }
}

#[allow(
    clippy::type_complexity,
    reason = "queue interruption reads existing activity owners"
)]
pub(super) fn cancel_interrupted_transport_queues(
    guests: Query<
        (
            Entity,
            &WaitingForTransport,
            &GuestPhase,
            Option<&GuestDestination>,
            Option<&GuestNavigationRequest>,
        ),
        With<Guest>,
    >,
    interruptions: Query<
        (),
        Or<(
            With<ServiceReservation>,
            With<BehaviorTaskExecutionState>,
            With<PendingBehaviorSet>,
        )>,
    >,
    mut stations: Query<&mut TransportStation>,
    mut commands: Commands,
) {
    for (guest, waiting, phase, destination, navigation) in &guests {
        if *phase == GuestPhase::Visiting
            && destination.is_none()
            && navigation.is_none_or(|request| request.0.is_none())
            && !interruptions.contains(guest)
        {
            continue;
        }
        if let Ok(mut station) = stations.get_mut(waiting.station) {
            station.queued = station.queued.saturating_sub(1);
        }
        commands.entity(guest).remove::<WaitingForTransport>();
    }
}
