use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::{
        facilities_and_maintenance::FacilityPaymentTrigger,
        guest_simulation_definitions::GuestVisitPurpose,
    },
    AssetId,
};

use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::zoo_entrance_anchor_synchronization::ZooEntrance;
use openzt2_zoo_state::economy::facility_economy_types::ServiceFacility;
use openzt2_zoo_state::economy::service_types::{PendingServiceRequest, ServiceRequestSequence};
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::NavFlags;
use openzt2_zoo_state::locomotion::locomotion_types::NavigateTo;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationFailure;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationRequestGeneration;
use openzt2_zoo_state::locomotion::locomotion_types::{EntranceNavigationConnection, SpatialGrid};

use super::{
    guest_destination_and_viewing_execution::{begin_viewing, cancel_failed_guest_destinations},
    guest_simulation_types::{Guest, GuestDestination, GuestNavigationRequest, GuestPhase},
};

fn spawn_map_grid(app: &mut App) {
    app.world_mut().spawn(SpatialGrid {
        width: 100,
        height: 100,
        ..Default::default()
    });
}

#[test]
fn only_current_guest_navigation_failure_releases_destination() {
    let mut app = App::new();
    app.add_message::<NavigationFailed>()
        .add_message::<NavigateTo>()
        .add_systems(Update, cancel_failed_guest_destinations);
    spawn_map_grid(&mut app);
    let destination = app.world_mut().spawn_empty().id();
    let guest = app
        .world_mut()
        .spawn((
            Guest,
            GuestNavigationRequest(Some(2)),
            GuestDestination {
                entity: destination,
                purpose: GuestVisitPurpose::View,
            },
        ))
        .id();
    app.world_mut().write_message(NavigationFailed {
        entity: guest,
        request_id: 1,
        reason: NavigationFailure::NoRoute,
    });
    app.update();
    assert!(app.world().get::<GuestDestination>(guest).is_some());
    assert_eq!(
        app.world().get::<GuestNavigationRequest>(guest).unwrap().0,
        Some(2)
    );
    app.world_mut().write_message(NavigationFailed {
        entity: guest,
        request_id: 2,
        reason: NavigationFailure::NoRoute,
    });
    app.update();
    assert!(app.world().get::<GuestDestination>(guest).is_none());
    assert_eq!(
        app.world().get::<GuestNavigationRequest>(guest).unwrap().0,
        None
    );
}

#[test]
fn failed_entrance_route_retries_without_admitting_or_accepting_stale_failures() {
    let mut app = App::new();
    app.add_message::<NavigationFailed>()
        .add_message::<NavigateTo>()
        .add_systems(Update, cancel_failed_guest_destinations);
    spawn_map_grid(&mut app);
    let root = app.world_mut().spawn_empty().id();
    let other_root = app.world_mut().spawn_empty().id();
    let entrance_position = Vec3::new(10.0, 0.0, 20.0);
    for (id, world_root, position) in [(1, other_root, Vec3::ZERO), (2, root, entrance_position)] {
        app.world_mut().spawn((
            PersistentId(id),
            WorldMember { root: world_root },
            ZooEntrance {
                arrival: position,
                inside: position,
                exit: position,
            },
        ));
    }
    let mut generation = NavigationRequestGeneration::default();
    let initial_request = generation.advance();
    let guest = app
        .world_mut()
        .spawn((
            Guest,
            GuestPhase::Arriving,
            GuestNavigationRequest(Some(initial_request)),
            generation,
            WorldMember { root },
            NavAgent {
                radius_m: 0.25,
                max_speed_mps: 1.0,
                acceleration_mps2: 1.0,
                capabilities: NavFlags::GUEST,
            },
            // Still walking in from the authored arrival point off the map.
            Transform::from_xyz(-80.0, 0.0, 20.0),
        ))
        .id();
    for _ in 0..2 {
        app.world_mut().write_message(NavigationFailed {
            entity: guest,
            request_id: initial_request,
            reason: NavigationFailure::NoStartNode,
        });
    }
    app.update();
    let requests: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<NavigateTo>>()
        .drain()
        .collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].entity, guest);
    assert_eq!(requests[0].destination, entrance_position);
    assert_eq!(requests[0].arrival_radius_m, 0.25);
    assert_ne!(requests[0].request_id, initial_request);
    assert_eq!(
        app.world().get::<GuestNavigationRequest>(guest).unwrap().0,
        Some(requests[0].request_id)
    );
    assert_eq!(
        app.world().get::<GuestPhase>(guest),
        Some(&GuestPhase::Arriving)
    );
    // The retried request keeps the walk from off the map to the gate.
    let connection = app
        .world()
        .get::<EntranceNavigationConnection>(guest)
        .unwrap();
    assert_eq!(connection.request_id, requests[0].request_id);
    assert_eq!(connection.gate, entrance_position);
    assert!(connection.inbound);
}

#[test]
fn current_arrival_hands_off_service_once_without_rechecking_render_transform() {
    let mut app = App::new();
    app.add_message::<Arrived>()
        .add_message::<NavigationFailed>()
        .add_systems(Update, begin_viewing);
    let root = app
        .world_mut()
        .spawn(ServiceRequestSequence::default())
        .id();
    let facility = app
        .world_mut()
        .spawn((
            Inspectable {
                definition: AssetId([1; 16]),
            },
            Visibility::Inherited,
            GlobalTransform::from_translation(Vec3::X * 10.0),
            ServiceFacility {
                definition: AssetId([1; 16]),
                capacity: 1,
                payment_trigger: FacilityPaymentTrigger::TimedTicks,
            },
        ))
        .id();
    let guest = app
        .world_mut()
        .spawn((
            Guest,
            openzt2_world_state::world_spawn::world_membership_types::WorldMember { root },
            GuestPhase::Visiting,
            GuestNavigationRequest(Some(2)),
            GuestDestination {
                entity: facility,
                purpose: GuestVisitPurpose::Food,
            },
            GlobalTransform::IDENTITY,
        ))
        .id();
    for request_id in [1, 2, 2] {
        app.world_mut().write_message(Arrived {
            entity: guest,
            request_id,
            target: None,
        });
    }
    app.update();
    assert_eq!(
        app.world()
            .get::<PendingServiceRequest>(guest)
            .unwrap()
            .sequence,
        0
    );
    assert_eq!(
        app.world()
            .get::<ServiceRequestSequence>(root)
            .unwrap()
            .next_sequence(),
        1
    );
    assert!(app.world().get::<GuestDestination>(guest).is_none());
    assert_eq!(
        app.world().get::<GuestNavigationRequest>(guest).unwrap().0,
        None
    );
}
