use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::{
        facilities_and_maintenance::FacilityPaymentTrigger,
        guest_simulation_definitions::GuestVisitPurpose,
    },
    AssetId,
};

use crate::plugins::{
    economy::{facility_economy_types::ServiceFacility, service_types::ServiceRequest},
    information::entity_selection_types::Inspectable,
    locomotion::locomotion_types::{
        Arrived, NavAgent, NavFlags, NavigateTo, NavigationFailed, NavigationFailure,
        NavigationRequestSequence,
    },
    world_spawn::{
        persistent_id_types::PersistentId, world_membership_types::WorldMember,
        zoo_entrance_anchor_synchronization::ZooEntrance,
    },
};

use super::{
    guest_destination_and_viewing_execution::{begin_viewing, cancel_failed_guest_destinations},
    guest_simulation_types::{Guest, GuestDestination, GuestNavigationRequest, GuestPhase},
};

#[test]
fn only_current_guest_navigation_failure_releases_destination() {
    let mut app = App::new();
    app.add_message::<NavigationFailed>()
        .add_message::<NavigateTo>()
        .init_resource::<NavigationRequestSequence>()
        .add_systems(Update, cancel_failed_guest_destinations);
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
        .init_resource::<NavigationRequestSequence>()
        .add_systems(Update, cancel_failed_guest_destinations);
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
    let initial_request = app
        .world_mut()
        .resource_mut::<NavigationRequestSequence>()
        .next();
    let guest = app
        .world_mut()
        .spawn((
            Guest,
            GuestPhase::Arriving,
            GuestNavigationRequest(Some(initial_request)),
            WorldMember { root },
            NavAgent {
                radius_m: 0.25,
                max_speed_mps: 1.0,
                acceleration_mps2: 1.0,
                capabilities: NavFlags::GUEST,
            },
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
}

#[test]
fn current_arrival_hands_off_service_once_without_rechecking_render_transform() {
    let mut app = App::new();
    app.add_message::<Arrived>()
        .add_message::<ServiceRequest>()
        .add_systems(Update, begin_viewing);
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
                occupied: 0,
                payment_trigger: FacilityPaymentTrigger::TimedTicks,
            },
        ))
        .id();
    let guest = app
        .world_mut()
        .spawn((
            Guest,
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
    assert_eq!(app.world().resource::<Messages<ServiceRequest>>().len(), 1);
    assert!(app.world().get::<GuestDestination>(guest).is_none());
    assert_eq!(
        app.world().get::<GuestNavigationRequest>(guest).unwrap().0,
        None
    );
}
