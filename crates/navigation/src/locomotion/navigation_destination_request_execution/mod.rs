use bevy::ecs::entity::EntityIndexMap;
use bevy::prelude::*;

use openzt2_zoo_state::locomotion::locomotion_types::Docking;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigateTo;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure;
use openzt2_zoo_state::locomotion::locomotion_types::{Destination, EntityPursuit};

pub(super) fn accept_navigation_destination_requests(
    mut commands: Commands,
    mut navigation_requests: MessageReader<NavigateTo>,
    mut navigation_agents: Query<Option<&mut Destination>, With<NavAgent>>,
    mut navigation_failures: MessageWriter<NavigationFailed>,
) {
    let mut inserted = EntityIndexMap::<Destination>::default();
    for navigation_request in navigation_requests.read() {
        let Ok(destination) = navigation_agents.get_mut(navigation_request.entity) else {
            navigation_failures.write(NavigationFailed {
                entity: navigation_request.entity,
                request_id: navigation_request.request_id,
                reason: NavigationFailure::NoRoute,
            });
            continue;
        };
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
        let next = Destination {
            request_id: navigation_request.request_id,
            world: navigation_request.destination,
            arrival_radius_m: navigation_request.arrival_radius_m,
        };
        if let Some(mut current) = destination {
            current.set_if_neq(next);
        } else {
            inserted.insert(navigation_request.entity, next);
        }
        commands
            .entity(navigation_request.entity)
            .remove::<(Docking, EntityPursuit)>();
    }
    for (actor, destination) in inserted {
        commands.entity(actor).insert(destination);
    }
}
