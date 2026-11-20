use bevy::{
    ecs::{
        lifecycle::{Despawn, Remove},
        relationship::RelationshipTarget,
    },
    prelude::*,
};

use openzt2_zoo_state::guests::guest_simulation_types::{Guest, GuestDeparted};

use super::{
    facility_economy_types::{Inventory, ServiceFacility},
    service_types::{
        FacilityReservations, ReservationFacility, ServiceOperation, ServicePhase,
        ServiceReservation,
    },
};

pub(super) fn release_removed_service_reservation(
    removed: On<Despawn<ServiceOperation>>,
    services: Query<(&ServiceOperation, &ReservationFacility)>,
    mut inventories: Query<&mut Inventory>,
) {
    let Ok((operation, facility)) = services.get(removed.entity) else {
        return;
    };
    if operation.inventory_units == 0 {
        return;
    }
    if let Ok(mut inventory) = inventories.get_mut(facility.0) {
        if operation.phase == ServicePhase::Completed {
            inventory.consume_reserved(operation.inventory_units);
        } else {
            inventory.return_reserved(operation.inventory_units);
        }
    }
}

pub(super) fn cancel_timed_reservations_when_facility_is_removed(
    removed: On<Remove<ServiceFacility>>,
    facilities: Query<&FacilityReservations>,
    mut commands: Commands,
) {
    if let Ok(reservations) = facilities.get(removed.entity) {
        for reservation in reservations.iter() {
            commands.entity(reservation).try_despawn();
        }
    }
}

pub(super) fn cancel_timed_reservation_when_guest_is_removed(
    removed: On<Remove<Guest>>,
    guests: Query<&ServiceReservation>,
    mut commands: Commands,
) {
    if let Ok(reservation) = guests.get(removed.entity) {
        for operation in reservation.iter() {
            commands.entity(operation).try_despawn();
        }
    }
}

pub(super) fn cancel_timed_service_for_departing_guests(
    mut departures: MessageReader<GuestDeparted>,
    guests: Query<&ServiceReservation>,
    mut commands: Commands,
) {
    for departure in departures.read() {
        if let Ok(reservation) = guests.get(departure.guest) {
            for operation in reservation.iter() {
                commands.entity(operation).try_despawn();
            }
        }
    }
}
