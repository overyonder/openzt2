use bevy::prelude::*;

use crate::plugins::guests::guest_simulation_types::{Guest, GuestDeparted};

use super::{
    facility_economy_types::{Inventory, ServiceFacility},
    service_capacity_and_inventory_release::release_service_capacity_and_settle_reserved_inventory,
    service_types::{ServicePaymentInFlight, ServiceProgress, ServiceReservation},
};

pub(crate) fn cancel_services_for_departed_guests_and_missing_facilities(
    customers: Query<(Entity, &ServiceReservation), With<ServiceProgress>>,
    mut facilities: Query<(&mut ServiceFacility, Option<&mut Inventory>)>,
    mut departed_guests: MessageReader<GuestDeparted>,
    mut removed_guest_components: RemovedComponents<Guest>,
    mut commands: Commands,
) {
    for departure in departed_guests.read() {
        let Ok((_, reservation)) = customers.get(departure.guest) else {
            continue;
        };
        release_service_capacity_and_settle_reserved_inventory(
            *reservation,
            false,
            &mut facilities,
        );
        commands.entity(departure.guest).remove::<(
            ServiceReservation,
            ServiceProgress,
            ServicePaymentInFlight,
        )>();
    }

    for customer in removed_guest_components.read() {
        let Ok((_, reservation)) = customers.get(customer) else {
            continue;
        };
        release_service_capacity_and_settle_reserved_inventory(
            *reservation,
            false,
            &mut facilities,
        );
        commands
            .entity(customer)
            .remove::<(ServiceReservation, ServiceProgress, ServicePaymentInFlight)>();
    }

    for (customer, reservation) in &customers {
        if facilities.get(reservation.facility).is_err() {
            commands
                .entity(customer)
                .remove::<(ServiceReservation, ServiceProgress, ServicePaymentInFlight)>();
        }
    }
}
