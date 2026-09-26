use bevy::prelude::*;

use crate::plugins::{
    guests::guest_simulation_types::{Guest, GuestArrived, GuestPhase, GuestReachedEntrance},
    world_spawn::world_membership_types::WorldMember,
};

use super::{
    account_transaction_settlement::settle_requested_account_transactions,
    account_transaction_types::{TransactionCompleted, TransactionRejected, TransactionRequest},
    facility_economy_types::Wallet,
    guest_admission_payment::{
        complete_guest_admission_after_successful_payment,
        reject_guest_admission_after_failed_payment,
        request_guest_admission_payment_after_entrance_reached,
    },
    guest_admission_types::{AdmissionPrice, ZooAdmissionsOpen},
    money_types::Money,
    zoo_cash_types::ZooCash,
};

fn create_application_with_guest_admission_payment(admission_price: Money) -> App {
    let mut application = App::new();
    application
        .add_message::<GuestReachedEntrance>()
        .add_message::<GuestArrived>()
        .add_message::<TransactionRequest>()
        .add_message::<TransactionCompleted>()
        .add_message::<TransactionRejected>()
        .insert_resource(AdmissionPrice(admission_price))
        .insert_resource(ZooAdmissionsOpen(true))
        .insert_resource(ZooCash(Money(1_000)))
        .add_systems(
            Update,
            (
                request_guest_admission_payment_after_entrance_reached,
                settle_requested_account_transactions,
                complete_guest_admission_after_successful_payment,
                reject_guest_admission_after_failed_payment,
            )
                .chain(),
        );
    application
}

fn spawn_arriving_guest_with_wallet(application: &mut App, wallet: Money) -> Entity {
    let world_root = application.world_mut().spawn_empty().id();
    application
        .world_mut()
        .spawn((
            Guest,
            GuestPhase::Arriving,
            Wallet(wallet),
            WorldMember { root: world_root },
        ))
        .id()
}

#[test]
fn repeated_paid_gate_arrivals_enter_visiting_after_one_atomic_payment() {
    let mut application = create_application_with_guest_admission_payment(Money(125));
    let guest = spawn_arriving_guest_with_wallet(&mut application, Money(500));
    application
        .world_mut()
        .write_message(GuestReachedEntrance { guest });
    application
        .world_mut()
        .write_message(GuestReachedEntrance { guest });

    application.update();

    assert_eq!(
        application.world().get::<GuestPhase>(guest),
        Some(&GuestPhase::Visiting)
    );
    assert_eq!(
        application.world().get::<Wallet>(guest),
        Some(&Wallet(Money(375)))
    );
    assert_eq!(application.world().resource::<ZooCash>().0, Money(1_125));
    assert_eq!(
        application
            .world()
            .resource::<Messages<GuestArrived>>()
            .len(),
        1
    );
}

#[test]
fn closed_admissions_turn_gate_arrival_away_without_payment() {
    let mut application = create_application_with_guest_admission_payment(Money(125));
    application
        .world_mut()
        .resource_mut::<ZooAdmissionsOpen>()
        .0 = false;
    let guest = spawn_arriving_guest_with_wallet(&mut application, Money(500));
    application
        .world_mut()
        .write_message(GuestReachedEntrance { guest });

    application.update();

    assert_eq!(
        application.world().get::<GuestPhase>(guest),
        Some(&GuestPhase::Leaving)
    );
    assert_eq!(
        application.world().get::<Wallet>(guest),
        Some(&Wallet(Money(500)))
    );
    assert_eq!(application.world().resource::<ZooCash>().0, Money(1_000));
    assert!(application
        .world()
        .resource::<Messages<GuestArrived>>()
        .is_empty());
}

#[test]
fn unaffordable_admission_preserves_balances_and_routes_guest_out() {
    let mut application = create_application_with_guest_admission_payment(Money(125));
    let guest = spawn_arriving_guest_with_wallet(&mut application, Money(100));
    application
        .world_mut()
        .write_message(GuestReachedEntrance { guest });

    application.update();

    assert_eq!(
        application.world().get::<GuestPhase>(guest),
        Some(&GuestPhase::Leaving)
    );
    assert_eq!(
        application.world().get::<Wallet>(guest),
        Some(&Wallet(Money(100)))
    );
    assert_eq!(application.world().resource::<ZooCash>().0, Money(1_000));
    assert!(application
        .world()
        .resource::<Messages<GuestArrived>>()
        .is_empty());
}
