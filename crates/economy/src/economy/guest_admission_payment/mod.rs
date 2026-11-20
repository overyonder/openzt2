use bevy::{ecs::relationship::RelationshipTarget, prelude::*};

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::economy::general_economy_types::GeneralEconomyTransaction;
use openzt2_zoo_state::guests::guest_simulation_types::{
    Guest, GuestArrived, GuestPhase, GuestReachedEntrance,
};

use super::{
    account_transaction_types::{
        Account, TransactionCompleted, TransactionKind, TransactionRejected, TransactionRequest,
    },
    facility_economy_types::Wallet,
    guest_admission_types::{
        AdmissionPaymentPending, AdmissionPrice, PendingGuestAdmission, ZooAdmissionsOpen,
    },
    money_types::Money,
};

pub(super) fn request_guest_admission_payment_after_entrance_reached(
    mut arrivals: MessageReader<GuestReachedEntrance>,
    admission_price: Res<AdmissionPrice>,
    admissions_open: Res<ZooAdmissionsOpen>,
    mut guests: Query<
        (&Wallet, &mut GuestPhase, &WorldMember),
        (With<Guest>, Without<PendingGuestAdmission>),
    >,
    mut commands: Commands,
    mut admitted: MessageWriter<GuestArrived>,
) {
    for arrival in arrivals.read() {
        let guest = arrival.guest;
        let Ok((wallet, mut phase, member)) = guests.get_mut(guest) else {
            continue;
        };
        if *phase != GuestPhase::Arriving {
            continue;
        }
        let amount = admission_price.0;
        if !admissions_open.0 || (amount != Money::ZERO && wallet.0 < amount) {
            *phase = GuestPhase::Leaving;
        } else if amount == Money::ZERO {
            *phase = GuestPhase::Visiting;
            admitted.write(GuestArrived { guest });
        } else {
            let member = *member;
            // Repeated arrivals commit in order against the guest's live payment link.
            commands.queue(move |world: &mut World| {
                if world.get::<GuestPhase>(guest) != Some(&GuestPhase::Arriving)
                    || world.get::<PendingGuestAdmission>(guest).is_some()
                {
                    return;
                }
                let operation = world
                    .spawn((
                        member,
                        AdmissionPaymentPending { guest },
                        GeneralEconomyTransaction::Admission,
                    ))
                    .id();
                world.write_message(TransactionRequest {
                    operation,
                    debit: Account::Entity(guest),
                    credit: Account::Zoo,
                    amount,
                    kind: TransactionKind::Admission,
                    subject: Some(guest),
                });
            });
        }
    }
}

pub(super) fn complete_guest_admission_after_successful_payment(
    mut completed: MessageReader<TransactionCompleted>,
    payments: Query<&AdmissionPaymentPending>,
    mut guests: Query<(&mut GuestPhase, &PendingGuestAdmission), With<Guest>>,
    mut commands: Commands,
    mut admitted: MessageWriter<GuestArrived>,
) {
    for transaction in completed.read() {
        let Ok(payment) = payments.get(transaction.operation) else {
            continue;
        };
        if let Ok((mut phase, pending)) = guests.get_mut(payment.guest) {
            if *phase == GuestPhase::Arriving
                && pending.iter().next() == Some(transaction.operation)
            {
                *phase = GuestPhase::Visiting;
                admitted.write(GuestArrived {
                    guest: payment.guest,
                });
            }
        }
        commands.entity(transaction.operation).try_despawn();
    }
}

pub(super) fn reject_guest_admission_after_failed_payment(
    mut rejected: MessageReader<TransactionRejected>,
    payments: Query<&AdmissionPaymentPending>,
    mut guests: Query<(&mut GuestPhase, &PendingGuestAdmission), With<Guest>>,
    mut commands: Commands,
) {
    for transaction in rejected.read() {
        let Ok(payment) = payments.get(transaction.operation) else {
            continue;
        };
        if let Ok((mut phase, pending)) = guests.get_mut(payment.guest) {
            if *phase == GuestPhase::Arriving
                && pending.iter().next() == Some(transaction.operation)
            {
                *phase = GuestPhase::Leaving;
            }
        }
        commands.entity(transaction.operation).try_despawn();
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
        let pending = app.world().get::<PendingGuestAdmission>(guest).unwrap();
        let payment = pending.iter().next().unwrap();
        assert_eq!(
            app.world().resource::<Messages<TransactionRequest>>().len(),
            1
        );
        app.world_mut()
            .write_message(GuestReachedEntrance { guest });
        app.update();
        assert_eq!(
            app.world().get::<GuestPhase>(guest),
            Some(&GuestPhase::Arriving)
        );
        let pending = app.world().get::<PendingGuestAdmission>(guest).unwrap();
        assert_eq!(pending.iter().next(), Some(payment));
    }
}
