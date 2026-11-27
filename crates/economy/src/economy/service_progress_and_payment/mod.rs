use bevy::prelude::*;
use openzt2_game_data::world_definitions::facilities_and_maintenance::FacilityPaymentTrigger;

use super::{
    account_transaction_types::{
        Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest,
    },
    facility_economy_types::{Price, ServiceFacility},
    money_types::Money,
    service_reservation::{FacilityServiceTimer, TimedFacilityService},
    service_types::{ReservationFacility, ServiceCompleted, ServiceCustomer},
};

pub(super) fn advance_reserved_services_and_request_completed_service_payments(
    mut services: Query<(
        Entity,
        &mut FacilityServiceTimer,
        &mut TimedFacilityService,
        &ServiceCustomer,
        &ReservationFacility,
    )>,
    facilities: Query<(&Price, &ServiceFacility)>,
    time: Res<Time<Fixed>>,
    mut commands: Commands,
    mut payments: MessageWriter<TransactionRequest>,
    mut completed: MessageWriter<ServiceCompleted>,
) {
    let step = time.timestep();
    for (entity, mut timer, mut operation, customer, facility) in &mut services {
        if !timer.0.is_finished() && !timer.0.tick(step).is_finished() {
            continue;
        }
        let Ok((price, service)) = facilities.get(facility.0) else {
            commands.entity(entity).try_despawn();
            continue;
        };
        if service.definition != operation.service
            || service.payment_trigger != FacilityPaymentTrigger::TimedTicks
        {
            warn!(customer = ?customer.0, facility = ?facility.0, "timed payment requested for a behavior-paid facility");
            commands.entity(entity).try_despawn();
        } else if price.0 == Money::ZERO {
            operation.completed = true;
            completed.write(ServiceCompleted {
                customer: customer.0,
                facility: facility.0,
                service: operation.service,
            });
            commands.entity(entity).try_despawn();
        } else {
            commands.entity(entity).try_remove::<FacilityServiceTimer>();
            payments.write(TransactionRequest {
                domain: openzt2_zoo_state::economy::account_transaction_types::TransactionDomain::Simulation,
                operation: entity,
                debit: Account::Entity(customer.0),
                credit: Account::Zoo,
                amount: price.0,
                kind: TransactionKind::Service,
                subject: Some(facility.0),
            });
        }
    }
}

pub(super) fn complete_services_after_successful_payments(
    mut receipts: MessageReader<TransactionCompleted>,
    mut services: Query<
        (
            &mut TimedFacilityService,
            &ServiceCustomer,
            &ReservationFacility,
        ),
        Without<FacilityServiceTimer>,
    >,
    mut commands: Commands,
    mut completed: MessageWriter<ServiceCompleted>,
) {
    for receipt in receipts.read() {
        let Ok((mut operation, customer, facility)) = services.get_mut(receipt.operation) else {
            continue;
        };
        if operation.completed {
            continue;
        }
        operation.completed = true;
        completed.write(ServiceCompleted {
            customer: customer.0,
            facility: facility.0,
            service: operation.service,
        });
        commands.entity(receipt.operation).try_despawn();
    }
}

pub(super) fn cancel_services_after_rejected_payments(
    mut rejections: MessageReader<TransactionRejected>,
    services: Query<(), (With<TimedFacilityService>, Without<FacilityServiceTimer>)>,
    mut commands: Commands,
) {
    for rejection in rejections.read() {
        if services.contains(rejection.operation) {
            commands.entity(rejection.operation).try_despawn();
        }
    }
}
