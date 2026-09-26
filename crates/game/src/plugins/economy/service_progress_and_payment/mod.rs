use bevy::prelude::*;
use openzt2_game_data::world_definitions::facilities_and_maintenance::FacilityPaymentTrigger;

use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    account_transaction_types::{
        Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest,
    },
    facility_economy_types::{Inventory, Price, ServiceFacility},
    money_types::Money,
    service_capacity_and_inventory_release::release_service_capacity_and_settle_reserved_inventory,
    service_types::{
        ServiceCompleted, ServicePaymentInFlight, ServicePaymentPending, ServiceProgress,
        ServiceReservation,
    },
};

pub(super) fn advance_reserved_services_and_request_completed_service_payments(
    mut customer_services: Query<(
        Entity,
        &WorldMember,
        &mut ServiceProgress,
        &ServiceReservation,
        Option<&ServicePaymentInFlight>,
    )>,
    mut facilities: Query<(&Price, &mut ServiceFacility, Option<&mut Inventory>)>,
    mut commands: Commands,
    mut transaction_requests: MessageWriter<TransactionRequest>,
    mut completed_services: MessageWriter<ServiceCompleted>,
) {
    for (customer, world_member, mut progress, reservation, pending_payment) in
        &mut customer_services
    {
        if pending_payment.is_some() {
            continue;
        }
        if progress.remaining_ticks > 0 {
            progress.remaining_ticks -= 1;
        }
        if progress.remaining_ticks != 0 {
            continue;
        }

        let Ok((price, mut facility, inventory)) = facilities.get_mut(reservation.facility) else {
            continue;
        };
        if facility.payment_trigger != FacilityPaymentTrigger::TimedTicks {
            // A reservation for a behavior-owned facility must never exist;
            // if one is manufactured anyway it must not be charged from a
            // timer. Cancel it without payment and keep the report explicit.
            warn!(
                customer = ?customer,
                facility = ?reservation.facility,
                "timed payment requested for a behavior-paid facility"
            );
            facility.release();
            if let Some(mut inventory) = inventory {
                inventory.return_reserved(reservation.inventory_units);
            }
            commands
                .entity(customer)
                .remove::<(ServiceReservation, ServiceProgress, ServicePaymentInFlight)>();
            continue;
        }
        if price.0 == Money::ZERO {
            facility.release();
            if let Some(mut inventory) = inventory {
                inventory.consume_reserved(reservation.inventory_units);
            }
            commands
                .entity(customer)
                .remove::<(ServiceReservation, ServiceProgress, ServicePaymentInFlight)>();
            completed_services.write(ServiceCompleted {
                customer,
                facility: reservation.facility,
                service: reservation.service,
            });
            continue;
        }

        let payment_operation = commands
            .spawn((
                *world_member,
                ServicePaymentPending {
                    customer,
                    facility: reservation.facility,
                    service: reservation.service,
                },
            ))
            .id();
        commands
            .entity(customer)
            .insert(ServicePaymentInFlight(payment_operation));
        transaction_requests.write(TransactionRequest {
            operation: payment_operation,
            debit: Account::Entity(customer),
            credit: Account::Zoo,
            amount: price.0,
            kind: TransactionKind::Service,
            subject: Some(reservation.facility),
        });
    }
}

pub(super) fn complete_services_after_successful_payments(
    mut completed_transactions: MessageReader<TransactionCompleted>,
    pending_payments: Query<&ServicePaymentPending>,
    customers: Query<&ServiceReservation>,
    mut facilities: Query<(&mut ServiceFacility, Option<&mut Inventory>)>,
    mut commands: Commands,
    mut completed_services: MessageWriter<ServiceCompleted>,
) {
    for transaction in completed_transactions.read() {
        let Ok(payment) = pending_payments.get(transaction.operation) else {
            continue;
        };
        let Ok(reservation) = customers.get(payment.customer) else {
            commands.entity(transaction.operation).despawn();
            continue;
        };
        if reservation.facility != payment.facility || reservation.service != payment.service {
            continue;
        }

        release_service_capacity_and_settle_reserved_inventory(*reservation, true, &mut facilities);
        commands.entity(payment.customer).remove::<(
            ServiceReservation,
            ServiceProgress,
            ServicePaymentInFlight,
        )>();
        commands.entity(transaction.operation).despawn();
        completed_services.write(ServiceCompleted {
            customer: payment.customer,
            facility: payment.facility,
            service: payment.service,
        });
    }
}

pub(super) fn cancel_services_after_rejected_payments(
    mut rejected_transactions: MessageReader<TransactionRejected>,
    pending_payments: Query<&ServicePaymentPending>,
    customers: Query<&ServiceReservation>,
    mut facilities: Query<(&mut ServiceFacility, Option<&mut Inventory>)>,
    mut commands: Commands,
) {
    for transaction in rejected_transactions.read() {
        let Ok(payment) = pending_payments.get(transaction.operation) else {
            continue;
        };
        if let Ok(reservation) = customers.get(payment.customer) {
            release_service_capacity_and_settle_reserved_inventory(
                *reservation,
                false,
                &mut facilities,
            );
            commands.entity(payment.customer).remove::<(
                ServiceReservation,
                ServiceProgress,
                ServicePaymentInFlight,
            )>();
        }
        commands.entity(transaction.operation).despawn();
    }
}

#[cfg(test)]
mod behavior_owned_service_settlement_gate_tests {
    use super::*;
    use crate::plugins::world_spawn::world_membership_types::WorldMember;
    use openzt2_game_data::AssetId;

    fn cart_facility_id() -> AssetId {
        AssetId::from_key("facility/snackcart_fruitcup_df")
    }

    /// One reserved guest at one placed cart. The cart carries a real reserved
    /// inventory so the cancellation path's inventory return is exercised.
    /// `payment_trigger` selects which gate the test exercises.
    fn create_application_with_reserved_guest(
        payment_trigger: FacilityPaymentTrigger,
    ) -> (App, Entity, Entity) {
        let mut application = App::new();
        application
            .add_message::<TransactionRequest>()
            .add_message::<ServiceCompleted>()
            .add_systems(
                Update,
                advance_reserved_services_and_request_completed_service_payments,
            );
        let cart = application.world_mut().spawn(()).id();
        let guest = application
            .world_mut()
            .spawn((
                WorldMember { root: cart },
                ServiceReservation {
                    facility: cart,
                    service: cart_facility_id(),
                    inventory_units: 2,
                },
                ServiceProgress {
                    facility: cart,
                    remaining_ticks: 0,
                },
            ))
            .id();
        application.world_mut().entity_mut(cart).insert((
            Price(Money(1_200)),
            ServiceFacility {
                definition: cart_facility_id(),
                capacity: 1,
                occupied: 1,
                payment_trigger,
            },
            Inventory {
                available: 3,
                reserved: 2,
                capacity: 5,
            },
        ));
        (application, cart, guest)
    }

    #[test]
    fn behavior_owned_reservation_is_cancelled_without_any_timer_payment() {
        let (mut application, cart, guest) =
            create_application_with_reserved_guest(FacilityPaymentTrigger::AuthoredBehavior);

        application.update();

        assert!(
            application
                .world()
                .resource::<Messages<TransactionRequest>>()
                .is_empty(),
            "a behavior-owned facility must never be charged from the timed service path"
        );
        assert!(application
            .world()
            .get::<ServiceReservation>(guest)
            .is_none());
        assert!(application.world().get::<ServiceProgress>(guest).is_none());
        assert_eq!(
            application
                .world()
                .get::<ServiceFacility>(cart)
                .unwrap()
                .occupied,
            0,
            "the manufactured reservation's capacity is released without payment"
        );
        let inventory = application.world().get::<Inventory>(cart).unwrap();
        assert_eq!(inventory.available, 5);
        assert_eq!(
            inventory.reserved, 0,
            "cancelled reservation returns its reserved stock"
        );
        assert!(
            application
                .world()
                .resource::<Messages<ServiceCompleted>>()
                .is_empty(),
            "canceling a behavior-owned reservation is not a completed service"
        );
    }

    #[test]
    fn timed_facility_service_still_requests_its_payment_from_the_timer() {
        let (mut application, cart, guest) =
            create_application_with_reserved_guest(FacilityPaymentTrigger::TimedTicks);

        application.update();

        let messages = application
            .world()
            .resource::<Messages<TransactionRequest>>();
        assert_eq!(messages.len(), 1);
        let (request, _) = messages.get_message(0).expect("one timed payment request");
        assert_eq!(request.amount, Money(1_200));
        assert_eq!(request.debit, Account::Entity(guest));
        assert_eq!(request.credit, Account::Zoo);
        assert_eq!(request.subject, Some(cart));
        assert!(
            application
                .world()
                .get::<ServicePaymentInFlight>(guest)
                .is_some(),
            "the timed path keeps its in-flight payment marker"
        );
        let inventory = application.world().get::<Inventory>(cart).unwrap();
        assert_eq!(inventory.available, 3);
        assert_eq!(
            inventory.reserved, 2,
            "reserved stock stays held until the payment settles"
        );
    }
}
