use bevy::prelude::*;

use super::{
    facility_economy_types::{Inventory, ServiceFacility},
    service_types::ServiceReservation,
};

pub(super) fn release_service_capacity_and_settle_reserved_inventory(
    reservation: ServiceReservation,
    service_completed: bool,
    facilities: &mut Query<(&mut ServiceFacility, Option<&mut Inventory>)>,
) {
    let Ok((mut facility, inventory)) = facilities.get_mut(reservation.facility) else {
        return;
    };
    facility.release();
    if let Some(mut inventory) = inventory {
        if service_completed {
            inventory.consume_reserved(reservation.inventory_units);
        } else {
            inventory.return_reserved(reservation.inventory_units);
        }
    }
}
