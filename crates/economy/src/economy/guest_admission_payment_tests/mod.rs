use bevy::prelude::*;
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};

use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::guests::guest_simulation_types::Guest;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArrived;
use openzt2_zoo_state::guests::guest_simulation_types::GuestPhase;
use openzt2_zoo_state::guests::guest_simulation_types::GuestReachedEntrance;

use super::{
    account_transaction_settlement::{
        settle_requested_account_transactions, stage_requested_account_transactions,
    },
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
        .init_resource::<WorldDefinitions>()
        .init_resource::<Assets<WorldDefinitionAsset>>()
        .add_message::<GuestReachedEntrance>()
        .add_message::<GuestArrived>()
        .add_message::<TransactionRequest>()
        .init_resource::<super::account_transaction_settlement::AccountRequestCursor>()
        .add_message::<openzt2_zoo_state::economy::account_transaction_types::RecordedAccountMovement<true>>()
        .add_message::<openzt2_zoo_state::economy::account_transaction_types::RecordedAccountMovement<false>>()
        .add_message::<TransactionCompleted>()
        .add_message::<TransactionRejected>()
        .add_systems(
            Update,
            (
                request_guest_admission_payment_after_entrance_reached,
                stage_requested_account_transactions::<false>,
                settle_requested_account_transactions::<false>,
                complete_guest_admission_after_successful_payment,
                reject_guest_admission_after_failed_payment,
            )
                .chain(),
        );
    application.world_mut().spawn((
        WorldRoot {
            scenario: Default::default(),
        },
        AdmissionPrice::from_adult(admission_price),
        ZooAdmissionsOpen(true),
        ZooCash(Money::from_cents(1_000)),
    ));
    application
}

fn zoo_root(application: &App) -> Entity {
    application
        .world()
        .iter_entities()
        .find(|entity| entity.contains::<WorldRoot>())
        .unwrap()
        .id()
}

fn spawn_arriving_guest_with_wallet(application: &mut App, wallet: Money) -> Entity {
    let world_root = zoo_root(application);
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
    let mut application = create_application_with_guest_admission_payment(Money::from_cents(125));
    let guest = spawn_arriving_guest_with_wallet(&mut application, Money::from_cents(500));
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
        Some(&Wallet(Money::from_cents(375)))
    );
    assert_eq!(
        application
            .world()
            .get::<ZooCash>(zoo_root(&application))
            .unwrap()
            .0,
        Money::from_cents(1_125)
    );
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
    let mut application = create_application_with_guest_admission_payment(Money::from_cents(125));
    let root = zoo_root(&application);
    application
        .world_mut()
        .get_mut::<ZooAdmissionsOpen>(root)
        .unwrap()
        .0 = false;
    let guest = spawn_arriving_guest_with_wallet(&mut application, Money::from_cents(500));
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
        Some(&Wallet(Money::from_cents(500)))
    );
    assert_eq!(
        application
            .world()
            .get::<ZooCash>(zoo_root(&application))
            .unwrap()
            .0,
        Money::from_cents(1_000)
    );
    assert!(application
        .world()
        .resource::<Messages<GuestArrived>>()
        .is_empty());
}

#[test]
fn unaffordable_admission_preserves_balances_and_routes_guest_out() {
    let mut application = create_application_with_guest_admission_payment(Money::from_cents(125));
    let guest = spawn_arriving_guest_with_wallet(&mut application, Money::from_cents(100));
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
        Some(&Wallet(Money::from_cents(100)))
    );
    assert_eq!(
        application
            .world()
            .get::<ZooCash>(zoo_root(&application))
            .unwrap()
            .0,
        Money::from_cents(1_000)
    );
    assert!(application
        .world()
        .resource::<Messages<GuestArrived>>()
        .is_empty());
}
