use bevy::{ecs::relationship::RelationshipTarget, platform::collections::HashSet, prelude::*};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::economy::general_economy_types::GeneralEconomyTransaction;
use openzt2_zoo_state::guests::guest_simulation_types::{
    Guest, GuestArchetype, GuestArrived, GuestPhase, GuestReachedEntrance,
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
    admission: Query<(&AdmissionPrice, &ZooAdmissionsOpen)>,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut guests: Query<
        (
            &Wallet,
            &mut GuestPhase,
            &WorldMember,
            Option<&GuestArchetype>,
        ),
        (With<Guest>, Without<PendingGuestAdmission>),
    >,
    mut commands: Commands,
    mut admitted: MessageWriter<GuestArrived>,
    mut payments: MessageWriter<TransactionRequest>,
) {
    if arrivals.is_empty() {
        return;
    }
    let definitions = definitions.get(&assets);
    let youth_types = ["youngmale", "youngfemale"].map(AssetId::from_key);
    let mut accepted_paid_guests = HashSet::new();
    for arrival in arrivals.read() {
        let guest = arrival.guest;
        let Ok((wallet, mut phase, member, archetype)) = guests.get_mut(guest) else {
            continue;
        };
        if *phase != GuestPhase::Arriving {
            continue;
        }
        let Ok((admission_price, admissions_open)) = admission.get(member.root) else {
            continue;
        };
        let is_child = archetype
            .and_then(|archetype| definitions?.find_guest(archetype.0))
            .is_some_and(|definition| {
                youth_types
                    .iter()
                    .any(|ty| definition.behavior_subject_type_identifiers.contains(ty))
            });
        let amount = if is_child {
            admission_price.child
        } else {
            admission_price.adult
        };
        if !admissions_open.0 || (amount != Money::ZERO && wallet.0 < amount) {
            *phase = GuestPhase::Leaving;
        } else if amount == Money::ZERO {
            *phase = GuestPhase::Visiting;
            admitted.write(GuestArrived { guest });
        } else if accepted_paid_guests.insert(guest) {
            let operation = commands
                .spawn((
                    *member,
                    AdmissionPaymentPending { guest },
                    GeneralEconomyTransaction::Admission,
                ))
                .id();
            payments.write(TransactionRequest {
                operation,
                debit: Account::Entity(guest),
                credit: Account::Zoo,
                amount,
                kind: TransactionKind::Admission,
                subject: Some(guest),
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
        app.init_resource::<WorldDefinitions>()
            .init_resource::<Assets<WorldDefinitionAsset>>()
            .add_message::<GuestReachedEntrance>()
            .add_message::<TransactionRequest>()
            .add_message::<GuestArrived>()
            .add_systems(
                Update,
                request_guest_admission_payment_after_entrance_reached,
            );
        let root = app
            .world_mut()
            .spawn((
                AdmissionPrice::from_adult(Money(100)),
                ZooAdmissionsOpen(true),
            ))
            .id();
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
