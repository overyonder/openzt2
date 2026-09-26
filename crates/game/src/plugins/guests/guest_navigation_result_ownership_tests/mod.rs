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
    locomotion::locomotion_types::{Arrived, NavigationFailed, NavigationFailure},
};

use super::{
    guest_destination_and_viewing_execution::{begin_viewing, cancel_failed_guest_destinations},
    guest_simulation_types::{Guest, GuestDestination, GuestNavigationRequest, GuestPhase},
};

#[test]
fn only_current_guest_navigation_failure_releases_destination() {
    let mut app = App::new();
    app.add_message::<NavigationFailed>()
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
