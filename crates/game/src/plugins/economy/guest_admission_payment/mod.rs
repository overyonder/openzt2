use bevy::{ecs::system::SystemParam, platform::collections::HashSet, prelude::*};

use crate::plugins::{
    guests::guest_simulation_types::{Guest, GuestArrived, GuestPhase, GuestReachedEntrance},
    world_spawn::world_membership_types::WorldMember,
};

use super::{
    account_transaction_types::{
        Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest,
    },
    facility_economy_types::Wallet,
    guest_admission_types::{AdmissionPaymentPending, AdmissionPrice, ZooAdmissionsOpen},
    money_types::Money,
};

#[derive(SystemParam)]
pub(super) struct GuestAdmissionRequests<'w, 's> {
    entrance_arrivals: MessageReader<'w, 's, GuestReachedEntrance>,
    pending_payments: Query<'w, 's, &'static AdmissionPaymentPending>,
    requested_guests: Local<'s, HashSet<Entity>>,
}

// Bevy extracts resource guards by value when invoking a scheduled system.
#[allow(clippy::needless_pass_by_value)]
pub(super) fn request_guest_admission_payment_after_entrance_reached(
    mut requests: GuestAdmissionRequests,
    admission_price: Res<AdmissionPrice>,
    admissions_open: Res<ZooAdmissionsOpen>,
    mut guests: Query<(&Wallet, &mut GuestPhase, &WorldMember), With<Guest>>,
    mut commands: Commands,
    mut transaction_requests: MessageWriter<TransactionRequest>,
    mut admitted_guests: MessageWriter<GuestArrived>,
) {
    requests.requested_guests.clear();
    let GuestAdmissionRequests {
        entrance_arrivals,
        pending_payments,
        requested_guests,
    } = &mut requests;
    for arrival in entrance_arrivals.read() {
        let Ok((wallet, mut phase, world_member)) = guests.get_mut(arrival.guest) else {
            continue;
        };
        if *phase != GuestPhase::Arriving {
            continue;
        }
        // Repeated arrival messages must neither charge twice before deferred
        // commands apply nor reject a guest whose payment is already underway.
        if !requested_guests.insert(arrival.guest)
            || pending_payments
                .iter()
                .any(|payment| payment.guest == arrival.guest)
        {
            continue;
        }
        if !admissions_open.0 {
            *phase = GuestPhase::Leaving;
            continue;
        }
        if admission_price.0 == Money::ZERO {
            *phase = GuestPhase::Visiting;
            admitted_guests.write(GuestArrived {
                guest: arrival.guest,
            });
            continue;
        }
        if wallet.0 < admission_price.0 {
            *phase = GuestPhase::Leaving;
            continue;
        }

        let transaction_operation = commands
            .spawn((
                *world_member,
                AdmissionPaymentPending {
                    guest: arrival.guest,
                },
            ))
            .id();
        transaction_requests.write(TransactionRequest {
            operation: transaction_operation,
            debit: Account::Entity(arrival.guest),
            credit: Account::Zoo,
            amount: admission_price.0,
            kind: TransactionKind::Admission,
            subject: Some(arrival.guest),
        });
    }
}

pub(super) fn complete_guest_admission_after_successful_payment(
    mut completed_transactions: MessageReader<TransactionCompleted>,
    pending_payments: Query<&AdmissionPaymentPending>,
    mut guests: Query<&mut GuestPhase, With<Guest>>,
    mut commands: Commands,
    mut admitted_guests: MessageWriter<GuestArrived>,
) {
    for transaction in completed_transactions.read() {
        let Ok(payment) = pending_payments.get(transaction.operation) else {
            continue;
        };
        if let Ok(mut phase) = guests.get_mut(payment.guest) {
            if *phase == GuestPhase::Arriving {
                *phase = GuestPhase::Visiting;
                admitted_guests.write(GuestArrived {
                    guest: payment.guest,
                });
            }
        }
        commands.entity(transaction.operation).despawn();
    }
}

pub(super) fn reject_guest_admission_after_failed_payment(
    mut rejected_transactions: MessageReader<TransactionRejected>,
    pending_payments: Query<&AdmissionPaymentPending>,
    mut guests: Query<&mut GuestPhase, With<Guest>>,
    mut commands: Commands,
) {
    for transaction in rejected_transactions.read() {
        let Ok(payment) = pending_payments.get(transaction.operation) else {
            continue;
        };
        if let Ok(mut phase) = guests.get_mut(payment.guest) {
            if *phase == GuestPhase::Arriving {
                *phase = GuestPhase::Leaving;
            }
        }
        commands.entity(transaction.operation).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_gate_arrivals_keep_one_pending_admission() {
        let mut app = App::new();
        app.insert_resource(AdmissionPrice(Money(100)))
            .insert_resource(ZooAdmissionsOpen(true))
            .add_message::<GuestReachedEntrance>()
            .add_message::<TransactionRequest>()
            .add_message::<GuestArrived>()
            .add_systems(
                Update,
                request_guest_admission_payment_after_entrance_reached,
            );
        let root = app.world_mut().spawn_empty().id();
        let guest = app
            .world_mut()
            .spawn((
                Guest,
                GuestPhase::Arriving,
                Wallet(Money(1000)),
                WorldMember { root },
            ))
            .id();
        for _ in 0..2 {
            app.world_mut()
                .write_message(GuestReachedEntrance { guest });
        }
        app.update();
        assert_eq!(
            app.world().resource::<Messages<TransactionRequest>>().len(),
            1
        );
        assert_eq!(
            app.world_mut()
                .query::<&AdmissionPaymentPending>()
                .iter(app.world())
                .count(),
            1
        );
        app.world_mut()
            .write_message(GuestReachedEntrance { guest });
        app.update();
        assert_eq!(
            app.world().get::<GuestPhase>(guest),
            Some(&GuestPhase::Arriving)
        );
        assert_eq!(
            app.world_mut()
                .query::<&AdmissionPaymentPending>()
                .iter(app.world())
                .count(),
            1
        );
    }
}
