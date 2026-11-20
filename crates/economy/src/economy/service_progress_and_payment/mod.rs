use bevy::prelude::*;
use openzt2_game_data::world_definitions::facilities_and_maintenance::FacilityPaymentTrigger;

use super::{
    account_transaction_types::{
        Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest,
    },
    facility_economy_types::{Price, ServiceFacility},
    money_types::Money,
    service_types::{
        ReservationFacility, ServiceCompleted, ServiceCustomer, ServiceOperation, ServicePhase,
    },
};

pub(super) fn advance_reserved_services_and_request_completed_service_payments(
    mut services: Query<(
        Entity,
        &mut ServiceOperation,
        &ServiceCustomer,
        &ReservationFacility,
    )>,
    facilities: Query<(&Price, &ServiceFacility)>,
    mut commands: Commands,
    mut payments: MessageWriter<TransactionRequest>,
    mut completed: MessageWriter<ServiceCompleted>,
) {
    for (entity, mut operation, customer, facility) in &mut services {
        let ServicePhase::Timing(remaining) = operation.phase else {
            continue;
        };
        if remaining > 1 {
            operation.phase = ServicePhase::Timing(remaining - 1);
            continue;
        }
        let Ok((price, service)) = facilities.get(facility.0) else {
            continue;
        };
        if service.payment_trigger != FacilityPaymentTrigger::TimedTicks {
            warn!(customer = ?customer.0, facility = ?facility.0, "timed payment requested for a behavior-paid facility");
            commands.entity(entity).try_despawn();
            continue;
        }
        if price.0 == Money::ZERO {
            operation.phase = ServicePhase::Completed;
            completed.write(ServiceCompleted {
                customer: customer.0,
                facility: facility.0,
                service: operation.service,
            });
            commands.entity(entity).try_despawn();
        } else {
            operation.phase = ServicePhase::AwaitingPayment;
            payments.write(TransactionRequest {
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
    mut services: Query<(
        &mut ServiceOperation,
        &ServiceCustomer,
        &ReservationFacility,
    )>,
    mut commands: Commands,
    mut completed: MessageWriter<ServiceCompleted>,
) {
    for receipt in receipts.read() {
        let Ok((mut operation, customer, facility)) = services.get_mut(receipt.operation) else {
            continue;
        };
        if operation.phase != ServicePhase::AwaitingPayment {
            continue;
        }
        operation.phase = ServicePhase::Completed;
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
    services: Query<&ServiceOperation>,
    mut commands: Commands,
) {
    for rejection in rejections.read() {
        if services
            .get(rejection.operation)
            .is_ok_and(|operation| operation.phase == ServicePhase::AwaitingPayment)
        {
            commands.entity(rejection.operation).try_despawn();
        }
    }
}
