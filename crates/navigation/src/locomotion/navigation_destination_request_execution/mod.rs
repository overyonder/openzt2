use bevy::prelude::*;

use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_zoo_state::locomotion::locomotion_types::Docking;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigateTo;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure;

pub(super) fn accept_navigation_destination_requests(
    mut commands: Commands,
    mut navigation_requests: MessageReader<NavigateTo>,
    navigation_agents: Query<(), With<NavAgent>>,
    mut navigation_failures: MessageWriter<NavigationFailed>,
) {
    for navigation_request in navigation_requests.read() {
        if navigation_agents.get(navigation_request.entity).is_err() {
            navigation_failures.write(NavigationFailed {
                entity: navigation_request.entity,
                request_id: navigation_request.request_id,
                reason: NavigationFailure::NoRoute,
            });
            continue;
        }
        if !navigation_request.destination.is_finite()
            || !navigation_request.arrival_radius_m.is_finite()
            || navigation_request.arrival_radius_m < 0.0
        {
            navigation_failures.write(NavigationFailed {
                entity: navigation_request.entity,
                request_id: navigation_request.request_id,
                reason: NavigationFailure::OutsideWorld,
            });
            continue;
        }
        commands
            .entity(navigation_request.entity)
            .insert(Destination {
                request_id: navigation_request.request_id,
                world: navigation_request.destination,
                arrival_radius_m: navigation_request.arrival_radius_m,
            });
        commands
            .entity(navigation_request.entity)
            .remove::<Docking>();
    }
}
