use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::guest_simulation_definitions::GuestVisitPurpose, AssetId,
};

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::{
    PendingBehaviorSet, StartBehaviorSet,
};
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::economy::service_types::ServiceReservation;
use openzt2_zoo_state::guests::guest_simulation_types::Guest;
use openzt2_zoo_state::guests::guest_simulation_types::GuestDestination;
use openzt2_zoo_state::guests::guest_simulation_types::GuestNavigationRequest;
use openzt2_zoo_state::guests::guest_simulation_types::GuestPhase;
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;

use openzt2_zoo_state::transport_tours::transport_rider_types::TransportRider;
use openzt2_zoo_state::transport_tours::transport_rider_types::WaitingForTransport;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation;

#[allow(
    clippy::type_complexity,
    reason = "guest ownership is expressed by ECS query filters"
)]
pub(super) fn start_authored_station_queue_for_guest_arrivals(
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
    mut behaviors: MessageWriter<StartBehaviorSet>,
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
            behaviors.write(StartBehaviorSet {
                actor: arrival.entity,
                program: AssetId::from_key("queuestation"),
                target: Some(destination.entity),
            });
        }
    }
}
