use bevy::{ecs::relationship::RelationshipTarget, platform::collections::HashMap, prelude::*};
use openzt2_game_data::{
    AssetId, world_definitions::facilities_and_maintenance::FacilityPaymentTrigger,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::{
    facility_economy_types::{Inventory, Price, ServiceFacility, Wallet},
    service_types::{
        FacilityReservations, PendingServiceRequest, ReservationFacility, ServiceCustomer,
        ServiceReservation,
    },
};

#[derive(Component)]
pub(super) struct TimedFacilityService {
    pub(super) service: AssetId,
    pub(super) inventory_units: u16,
    pub(super) completed: bool,
}

#[derive(Component)]
pub(super) struct FacilityServiceTimer(pub(super) Timer);

pub(super) fn reserve_requested_facility_capacity_and_inventory_for_customers(
    requests: Query<(
        Entity,
        &PendingServiceRequest,
        Option<&Wallet>,
        Option<&WorldMember>,
        Option<&ServiceReservation>,
    )>,
    facilities: Query<(&Price, &ServiceFacility, Option<&FacilityReservations>)>,
    mut inventories: Query<&mut Inventory>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    time: Res<Time<Fixed>>,
    mut commands: Commands,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let mut ordered_requests = requests.iter().collect::<Vec<_>>();
    ordered_requests.sort_unstable_by_key(|(customer, request, ..)| (request.sequence, *customer));
    let mut accepted_by_facility = HashMap::<Entity, usize>::new();
    for (customer, request, wallet, member, reservation) in ordered_requests {
        commands
            .entity(customer)
            .try_remove::<PendingServiceRequest>();
        let Some(definition) = definitions.find_facility(request.service) else {
            continue;
        };
        let facility = request.facility;
        let (Some(wallet), Ok((price, service, reservations))) = (wallet, facilities.get(facility))
        else {
            continue;
        };
        if definition.payment_trigger != FacilityPaymentTrigger::TimedTicks
            || service.payment_trigger != FacilityPaymentTrigger::TimedTicks
            || service.definition != request.service
        {
            warn!(?customer, ?facility, requested_service = ?request.service, "live facility does not provide the requested timed service");
            continue;
        }
        if price.0.0 < 0
            || wallet.0 < price.0
            || reservation.is_some_and(|reservation| !reservation.is_empty())
        {
            continue;
        }
        let accepted = accepted_by_facility.entry(facility).or_default();
        if reservations.map_or(0, RelationshipTarget::len) + *accepted
            >= usize::from(service.capacity)
        {
            continue;
        }
        let inventory_units = definition.inventory_units_per_service;
        if inventory_units > 0
            && !inventories
                .get_mut(facility)
                .is_ok_and(|mut stock| stock.reserve(inventory_units))
        {
            continue;
        }
        *accepted += 1;
        let mut operation = commands.spawn((
            ServiceCustomer(customer),
            ReservationFacility(facility),
            TimedFacilityService {
                service: request.service,
                inventory_units,
                completed: false,
            },
            FacilityServiceTimer(Timer::new(
                time.timestep().saturating_mul(definition.service_ticks),
                TimerMode::Once,
            )),
        ));
        if let Some(member) = member {
            operation.insert(*member);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::world_definitions::{
        document::WorldDefinitionDocument,
        facilities_and_maintenance::{FacilityDefinition, FacilityServiceKind},
        staff_management::StaffRoleKind,
    };
    use openzt2_zoo_state::economy::money_types::Money;

    #[test]
    fn simultaneous_requests_reserve_stock_for_the_earliest_eligible_customer() {
        let mut app = App::new();
        app.init_resource::<Assets<WorldDefinitionAsset>>()
            .init_resource::<WorldDefinitions>()
            .init_resource::<Time<Fixed>>()
            .add_systems(
                Update,
                reserve_requested_facility_capacity_and_inventory_for_customers,
            )
            .add_observer(super::super::service_cancellation::release_removed_service_reservation);
        let world = app.world_mut();
        let definition = AssetId::from_key("facility/test_diner");
        let document = WorldDefinitionDocument {
            facilities: vec![FacilityDefinition {
                id: definition,
                object: AssetId::from_key("test_diner"),
                service: FacilityServiceKind::Food,
                capacity: 1,
                service_ticks: 3,
                payment_trigger: FacilityPaymentTrigger::TimedTicks,
                price_cents: 20,
                staffing: StaffRoleKind::None,
                inventory_capacity: 5,
                inventory_units_per_service: 2,
                inventory_restock_per_zoo_day: 0,
            }],
            simulation_timing: Some(
                super::super::authored_economy_fact_hydration::economy_test_simulation_timing(),
            ),
            ..default()
        };
        let handle = world
            .resource_mut::<Assets<WorldDefinitionAsset>>()
            .add(WorldDefinitionAsset::from_test_document(document.clone()));
        world
            .resource_mut::<WorldDefinitions>()
            .index_test_document(handle, &document);
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
        let request = PendingServiceRequest {
            sequence: 0,
            facility,
            service: definition,
        };
        let first = world.spawn((Wallet(Money(40)), request)).id();
        let second = world.spawn(Wallet(Money(40))).id();
        app.update();
        let world = app.world_mut();
        assert!(world.get::<ServiceReservation>(first).is_none());
        assert_eq!(world.get::<Inventory>(facility).unwrap().available, 5);
        world
            .get_mut::<ServiceFacility>(facility)
            .unwrap()
            .payment_trigger = FacilityPaymentTrigger::TimedTicks;
        world.entity_mut(first).insert(request);
        world.entity_mut(second).insert(PendingServiceRequest {
            sequence: 1,
            ..request
        });
        app.update();
        let world = app.world_mut();
        assert_eq!(
            world.get::<FacilityReservations>(facility).unwrap().len(),
            1
        );
        assert!(world.get::<ServiceReservation>(second).is_none());
        assert_eq!(world.get::<Inventory>(facility).unwrap().available, 3);
        world.despawn(first);
        world.flush();
        assert!(world.get::<FacilityReservations>(facility).is_none());
        assert_eq!(world.get::<Inventory>(facility).unwrap().available, 5);
    }
}
