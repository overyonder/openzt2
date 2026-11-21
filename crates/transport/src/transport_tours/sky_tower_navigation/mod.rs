use bevy::prelude::*;

use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavigateTo;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationRequestSequence;

use openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitMember;
use openzt2_zoo_state::transport_tours::transport_topology_types::SkyTowerTransitionRequest;
use openzt2_zoo_state::transport_tours::transport_topology_types::{
    SkyTowerEndpoint, SkyTowerEndpoints, SkyTowerHeight,
};

/// Resolves the sky-tower up/down action through the tower's
/// authored topology endpoints and ordinary navigation. This never moves the
/// rider directly or infers a height from the rendered tower.
pub(super) fn request_sky_tower_rider_navigation_to_authored_endpoint(
    mut transition_requests: MessageReader<SkyTowerTransitionRequest>,
    sky_towers: Query<(), (With<SkyTowerHeight>, With<CircuitMember>)>,
    endpoints: SkyTowerEndpoints,
    transport_riders: Query<&NavAgent>,
    mut navigation_request_sequence: ResMut<NavigationRequestSequence>,
    mut navigation_requests: MessageWriter<NavigateTo>,
) {
    for transition_request in transition_requests.read() {
        if !sky_towers.contains(transition_request.tower) {
            continue;
        }
        let end = if transition_request.travel_to_upper_endpoint {
            SkyTowerEndpoint::Upper
        } else {
            SkyTowerEndpoint::Lower
        };
        let Some(destination) = endpoints.position(transition_request.tower, end) else {
            continue;
        };
        let Ok(rider_nav_agent) = transport_riders.get(transition_request.rider) else {
            continue;
        };
        navigation_requests.write(NavigateTo {
            entity: transition_request.rider,
            request_id: navigation_request_sequence.next(),
            destination,
            arrival_radius_m: rider_nav_agent.radius_m,
        });
    }
}

pub(super) fn hydrate_sky_tower_endpoints(
    mut commands: Commands,
    towers: Query<(Entity, &SkyTowerHeight, &Transform), Added<SkyTowerHeight>>,
) {
    for (tower, height, transform) in &towers {
        for (end, local) in [
            (SkyTowerEndpoint::Lower, Vec3::ZERO),
            (SkyTowerEndpoint::Upper, Vec3::Y * height.0),
        ] {
            commands.spawn((
                end,
                Transform::from_translation(local),
                GlobalTransform::from_translation(transform.transform_point(local)),
                ChildOf(tower),
            ));
        }
    }
}

pub(super) fn authored_sky_tower_height(
    track: &openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackDefinition,
    from: Vec3,
    base: Vec3,
    steps: i16,
) -> Option<f32> {
    let presentation = track.sky_tower_presentation.as_ref()?;
    let increment = presentation.columns[1].repeating_height_metres;
    let height = (presentation.initial_height_metres + f32::from(steps) * increment)
        .max(presentation.minimum_height_metres)
        .max(from.y - base.y + track.sky_rope_clearance_metres.unwrap_or_default());
    (height.is_finite() && height > 0.0).then_some(height)
}
