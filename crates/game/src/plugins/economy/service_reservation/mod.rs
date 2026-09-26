use bevy::{platform::collections::HashSet, prelude::*};
use openzt2_game_data::world_definitions::facilities_and_maintenance::FacilityPaymentTrigger;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::{
    facility_economy_types::{Inventory, Price, ServiceFacility, Wallet},
    service_types::{ServiceProgress, ServiceRequest, ServiceReservation},
};

pub(super) fn reserve_requested_facility_capacity_and_inventory_for_customers(
    mut service_requests: MessageReader<ServiceRequest>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    customers: Query<(&Wallet, Option<&ServiceReservation>)>,
    mut facilities: Query<(&Price, &mut ServiceFacility, Option<&mut Inventory>)>,
    mut commands: Commands,
    mut customers_accepted_this_tick: Local<HashSet<Entity>>,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    customers_accepted_this_tick.clear();

    for request in service_requests.read() {
        let Ok((wallet, existing_reservation)) = customers.get(request.customer) else {
            continue;
        };
        if existing_reservation.is_some()
            || customers_accepted_this_tick.contains(&request.customer)
        {
            continue;
        }
        let Some(service_definition) = world_definitions.find_facility(request.service) else {
            continue;
        };
        if service_definition.payment_trigger != FacilityPaymentTrigger::TimedTicks {
            // The guest buy behavior handles these services.
            warn!(
                customer = ?request.customer,
                facility = ?request.facility,
                "timed service requested for a behavior-paid facility"
            );
            continue;
        }
        let Ok((price, mut facility, inventory)) = facilities.get_mut(request.facility) else {
            continue;
        };

        let inventory_units = service_definition.inventory_units_per_service;
        // The requested service must match the live facility.
        if facility.payment_trigger != FacilityPaymentTrigger::TimedTicks {
            warn!(
                customer = ?request.customer,
                facility = ?request.facility,
                requested_service = ?request.service,
                "timed reservation requested for a behavior-paid facility"
            );
            continue;
        }
        if facility.definition != request.service {
            warn!(
                customer = ?request.customer,
                facility = ?request.facility,
                requested_service = ?request.service,
                "live facility does not provide the requested service definition; refusing timed reservation"
            );
            continue;
        }
        if price.0 .0 < 0 || wallet.0 < price.0 {
            continue;
        }
        let mut inventory = inventory;
        if inventory_units > 0
            && inventory
                .as_ref()
                .is_none_or(|stock| !stock.can_reserve(inventory_units))
        {
            continue;
        }
        if !facility.try_acquire() {
            continue;
        }
        if let Some(stock) = inventory.as_mut() {
            if !stock.reserve(inventory_units) {
                facility.release();
                continue;
            }
        }

        commands.entity(request.customer).insert((
            ServiceReservation {
                facility: request.facility,
                service: request.service,
                inventory_units,
            },
            ServiceProgress {
                facility: request.facility,
                remaining_ticks: service_definition.service_ticks,
            },
        ));
        customers_accepted_this_tick.insert(request.customer);
    }
}

#[cfg(test)]
mod behavior_owned_service_reservation_gate_tests {
    use super::*;
    use crate::plugins::economy::money_types::Money;
    use openzt2_game_data::world_definitions::{
        document::WorldDefinitionDocument,
        facilities_and_maintenance::{FacilityDefinition, FacilityServiceKind},
        staff_management::StaffRoleKind,
    };
    use openzt2_game_data::AssetId;

    fn cart_facility_id() -> AssetId {
        AssetId::from_key("facility/snackcart_fruitcup_df")
    }

    fn timed_diner_id() -> AssetId {
        AssetId::from_key("facility/test_diner")
    }

    fn authored_cart_definition() -> FacilityDefinition {
        FacilityDefinition {
            id: cart_facility_id(),
            object: AssetId::from_key("snackcart_fruitcup_df"),
            service: FacilityServiceKind::Food,
            capacity: 1,
            service_ticks: 0,
            payment_trigger: FacilityPaymentTrigger::AuthoredBehavior,
            price_cents: 1_200,
            staffing: StaffRoleKind::None,
            inventory_capacity: 0,
            inventory_units_per_service: 0,
            inventory_restock_per_zoo_day: 0,
        }
    }

    fn timed_diner_definition() -> FacilityDefinition {
        FacilityDefinition {
            id: timed_diner_id(),
            object: AssetId::from_key("test_diner"),
            service: FacilityServiceKind::Food,
            capacity: 1,
            service_ticks: 30,
            payment_trigger: FacilityPaymentTrigger::TimedTicks,
            price_cents: 500,
            staffing: StaffRoleKind::None,
            inventory_capacity: 0,
            inventory_units_per_service: 0,
            inventory_restock_per_zoo_day: 0,
        }
    }

    /// Installs facility documents with one authored cart and one timed diner,
    /// a placed facility entity, and one paying guest. Each test decides which
    /// service id is requested and which live facts the entity declares.
    fn create_application_with_facility_documents() -> (App, Entity, Entity) {
        // The system reads only the Assets resource; no AssetServer or asset
        // plugin is involved, matching the established test fixture pattern.
        let mut application = App::new();
        application
            .add_message::<ServiceRequest>()
            .insert_resource(Assets::<WorldDefinitionAsset>::default())
            .init_resource::<WorldDefinitions>()
            .add_systems(
                Update,
                reserve_requested_facility_capacity_and_inventory_for_customers,
            );
        let document = WorldDefinitionDocument {
            facilities: vec![authored_cart_definition(), timed_diner_definition()],
            ..default()
        };
        let handle = application
            .world_mut()
            .resource_mut::<Assets<WorldDefinitionAsset>>()
            .add(WorldDefinitionAsset::from_test_document(document.clone()));
        application
            .world_mut()
            .resource_mut::<WorldDefinitions>()
            .index_test_document(handle, &document);
        let cart = application.world_mut().spawn(()).id();
        let guest = application.world_mut().spawn(Wallet(Money(5_000))).id();
        (application, cart, guest)
    }

    fn insert_live_facility(
        application: &mut App,
        facility_entity: Entity,
        definition: AssetId,
        payment_trigger: FacilityPaymentTrigger,
    ) {
        application.world_mut().entity_mut(facility_entity).insert((
            Price(Money(1_200)),
            ServiceFacility {
                definition,
                capacity: 1,
                occupied: 0,
                payment_trigger,
            },
        ));
    }

    fn write_service_request(
        application: &mut App,
        facility_entity: Entity,
        guest: Entity,
        service: AssetId,
    ) {
        application.world_mut().write_message(ServiceRequest {
            customer: guest,
            facility: facility_entity,
            service,
        });
    }

    #[test]
    fn behavior_owned_service_request_is_never_reserved_or_charged_by_the_timer_path() {
        let (mut application, cart, guest) = create_application_with_facility_documents();
        insert_live_facility(
            &mut application,
            cart,
            cart_facility_id(),
            FacilityPaymentTrigger::AuthoredBehavior,
        );
        write_service_request(&mut application, cart, guest, cart_facility_id());

        application.update();

        assert!(
            application
                .world()
                .get::<ServiceReservation>(guest)
                .is_none(),
            "a behavior-owned facility must not be claimed by the timed reservation path"
        );
        assert!(application.world().get::<ServiceProgress>(guest).is_none());
        assert_eq!(
            application
                .world()
                .get::<ServiceFacility>(cart)
                .unwrap()
                .occupied,
            0,
            "service capacity must not be acquired for behavior-owned payment"
        );
    }

    #[test]
    fn timed_service_id_cannot_reserve_a_behavior_owned_live_facility() {
        let (mut application, cart, guest) = create_application_with_facility_documents();
        // The timed definition exists in the catalogue, but the targeted live
        // entity declares behavior-owned payment.
        insert_live_facility(
            &mut application,
            cart,
            timed_diner_id(),
            FacilityPaymentTrigger::AuthoredBehavior,
        );
        write_service_request(&mut application, cart, guest, timed_diner_id());

        application.update();

        assert!(
            application.world().get::<ServiceReservation>(guest).is_none(),
            "a timed service id must not reserve a live entity that declares behavior-owned payment"
        );
        assert_eq!(
            application
                .world()
                .get::<ServiceFacility>(cart)
                .unwrap()
                .occupied,
            0
        );
    }

    #[test]
    fn service_id_mismatching_the_live_facility_definition_is_refused() {
        let (mut application, cart, guest) = create_application_with_facility_documents();
        // The entity claims the authored cart while the guest requests the
        // timed diner: the definition lookup succeeds but the live facts do
        // not match the requested service.
        insert_live_facility(
            &mut application,
            cart,
            cart_facility_id(),
            FacilityPaymentTrigger::TimedTicks,
        );
        write_service_request(&mut application, cart, guest, timed_diner_id());

        application.update();

        assert!(
            application
                .world()
                .get::<ServiceReservation>(guest)
                .is_none(),
            "a timed reservation must require the live definition to match the requested service"
        );
        assert_eq!(
            application
                .world()
                .get::<ServiceFacility>(cart)
                .unwrap()
                .occupied,
            0
        );
    }

    #[test]
    fn timed_facility_service_still_reserves_capacity_for_its_timer() {
        let (mut application, cart, guest) = create_application_with_facility_documents();
        insert_live_facility(
            &mut application,
            cart,
            timed_diner_id(),
            FacilityPaymentTrigger::TimedTicks,
        );
        write_service_request(&mut application, cart, guest, timed_diner_id());

        application.update();

        let reservation = application
            .world()
            .get::<ServiceReservation>(guest)
            .expect("a timed facility keeps its reservation path");
        assert_eq!(reservation.facility, cart);
        assert_eq!(reservation.service, timed_diner_id());
        assert_eq!(
            application
                .world()
                .get::<ServiceFacility>(cart)
                .unwrap()
                .occupied,
            1
        );
    }
}
