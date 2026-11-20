use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_data::world_definitions::facilities_and_maintenance::FacilityPaymentTrigger;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::{
    facility_economy_types::{Inventory, Price, ServiceFacility, Wallet},
    service_types::{
        FacilityReservations, PendingServiceRequest, ReservationFacility, ServiceCustomer,
        ServiceOperation, ServicePhase, ServiceReservation,
    },
};

pub(super) fn reserve_requested_facility_capacity_and_inventory_for_customers(
    requests: Query<(Entity, &PendingServiceRequest)>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut commands: Commands,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let mut ordered_requests = requests.iter().collect::<Vec<_>>();
    ordered_requests.sort_unstable_by_key(|(customer, request)| (request.sequence, *customer));
    for (customer, request) in ordered_requests {
        commands
            .entity(customer)
            .try_remove::<PendingServiceRequest>();
        let Some(definition) = definitions.find_facility(request.service) else {
            continue;
        };
        if definition.payment_trigger != FacilityPaymentTrigger::TimedTicks {
            warn!(?customer, facility = ?request.facility, "timed service requested for a behavior-paid facility");
            continue;
        }
        let request = *request;
        let inventory_units = definition.inventory_units_per_service;
        let duration = definition.service_ticks;
        // Requests competing for the same facility must see the previous
        // request's newly created reservation and reserved stock.
        commands.queue(move |world: &mut World| {
            commit_timed_service_reservation(world, customer, request, inventory_units, duration);
        });
    }
}

fn commit_timed_service_reservation(
    world: &mut World,
    customer: Entity,
    request: PendingServiceRequest,
    inventory_units: u16,
    duration: u32,
) {
    let facility = request.facility;
    let Some((price, service, wallet)) = world
        .get::<Price>(facility)
        .zip(world.get::<ServiceFacility>(facility))
        .zip(world.get::<Wallet>(customer))
        .map(|((price, service), wallet)| (price.0, *service, wallet.0))
    else {
        return;
    };
    if service.payment_trigger != FacilityPaymentTrigger::TimedTicks
        || service.definition != request.service
    {
        warn!(?customer, ?facility, requested_service = ?request.service, "live facility does not provide the requested timed service");
        return;
    }
    if price.0 < 0
        || wallet < price
        || world
            .get::<ServiceReservation>(customer)
            .is_some_and(|reservation| !reservation.is_empty())
        || world
            .get::<FacilityReservations>(facility)
            .map_or(0, RelationshipTarget::len)
            >= usize::from(service.capacity)
    {
        return;
    }
    if inventory_units > 0
        && !world
            .get_mut::<Inventory>(facility)
            .is_some_and(|mut stock| stock.reserve(inventory_units))
    {
        return;
    }
    let member = world.get::<WorldMember>(customer).copied();
    let mut operation = world.spawn((
        ServiceCustomer(customer),
        ReservationFacility(facility),
        ServiceOperation {
            service: request.service,
            inventory_units,
            phase: ServicePhase::Timing(duration),
        },
    ));
    if let Some(member) = member {
        operation.insert(member);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::AssetId;
    use openzt2_zoo_state::economy::money_types::Money;

    #[test]
    fn reservation_checks_live_facility_and_recovers_stock_when_customer_despawns() {
        let mut app = App::new();
        app.add_observer(super::super::service_cancellation::release_removed_service_reservation);
        let world = app.world_mut();
        let definition = AssetId::from_key("facility/test_diner");
        let facility = world
            .spawn((
                Price(Money(20)),
                ServiceFacility {
                    definition,
                    capacity: 1,
                    payment_trigger: FacilityPaymentTrigger::AuthoredBehavior,
                },
                Inventory {
                    available: 5,
                    reserved: 0,
                    capacity: 5,
                },
            ))
            .id();
        let guest = world.spawn(Wallet(Money(40))).id();
        let other_guest = world.spawn(Wallet(Money(40))).id();
        let request = PendingServiceRequest {
            sequence: 0,
            facility,
            service: definition,
        };
        commit_timed_service_reservation(world, guest, request, 2, 3);
        assert!(world.get::<ServiceReservation>(guest).is_none());
        world
            .get_mut::<ServiceFacility>(facility)
            .unwrap()
            .payment_trigger = FacilityPaymentTrigger::TimedTicks;
        commit_timed_service_reservation(world, guest, request, 2, 3);
        commit_timed_service_reservation(world, other_guest, request, 2, 3);
        assert_eq!(
            world.get::<FacilityReservations>(facility).unwrap().len(),
            1
        );
        assert!(world.get::<ServiceReservation>(other_guest).is_none());
        assert_eq!(world.get::<Inventory>(facility).unwrap().available, 3);
        world.despawn(guest);
        world.flush();
        assert!(world.get::<FacilityReservations>(facility).is_none());
        assert_eq!(world.get::<Inventory>(facility).unwrap().available, 5);
    }
}
