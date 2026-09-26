use bevy::prelude::*;

use crate::plugins::locomotion::locomotion_types::{
    NavAgent, NavigateTo, NavigationRequestSequence,
};

use super::{
    transport_circuit_types::CircuitMember,
    transport_topology_types::{SkyTowerEndpoints, SkyTowerTransitionRequest},
};

/// Resolves the sky-tower up/down action through the tower's
/// authored topology endpoints and ordinary navigation. This never moves the
/// rider directly or infers a height from the rendered tower.
pub(super) fn request_sky_tower_rider_navigation_to_authored_endpoint(
    mut transition_requests: MessageReader<SkyTowerTransitionRequest>,
    sky_towers: Query<(&SkyTowerEndpoints, &CircuitMember)>,
    authored_endpoints: Query<(&GlobalTransform, &CircuitMember)>,
    transport_riders: Query<&NavAgent>,
    mut navigation_request_sequence: ResMut<NavigationRequestSequence>,
    mut navigation_requests: MessageWriter<NavigateTo>,
) {
    for transition_request in transition_requests.read() {
        let Ok((sky_tower_endpoints, tower_circuit_member)) =
            sky_towers.get(transition_request.tower)
        else {
            continue;
        };
        let destination_endpoint = if transition_request.travel_to_upper_endpoint {
            sky_tower_endpoints.upper
        } else {
            sky_tower_endpoints.lower
        };
        let Ok((destination_transform, endpoint_circuit_member)) =
            authored_endpoints.get(destination_endpoint)
        else {
            continue;
        };
        let Ok(rider_nav_agent) = transport_riders.get(transition_request.rider) else {
            continue;
        };
        if endpoint_circuit_member.0 != tower_circuit_member.0 {
            continue;
        }
        navigation_requests.write(NavigateTo {
            entity: transition_request.rider,
            request_id: navigation_request_sequence.next(),
            destination: destination_transform.translation(),
            arrival_radius_m: rider_nav_agent.radius_m,
        });
    }
}
