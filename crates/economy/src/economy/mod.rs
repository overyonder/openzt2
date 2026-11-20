use openzt2_zoo_state::economy::account_transaction_types;
use openzt2_zoo_state::economy::authored_price_queries;
use openzt2_zoo_state::economy::authored_transaction_amount;
use openzt2_zoo_state::economy::behavior_transaction_state;
use openzt2_zoo_state::economy::currency_formatting;
use openzt2_zoo_state::economy::facility_economy_types;
use openzt2_zoo_state::economy::general_economy_types::GeneralEconomyLedger;
use openzt2_zoo_state::economy::guest_admission_types;
use openzt2_zoo_state::economy::money_types;
use openzt2_zoo_state::economy::monthly_finance_types;
use openzt2_zoo_state::economy::object_sale_refund;
use openzt2_zoo_state::economy::scenario_economy_command_types;
use openzt2_zoo_state::economy::service_types;
use openzt2_zoo_state::economy::zoo_cash_types;
mod account_transaction_settlement;
pub mod authored_economy_fact_hydration;
mod behavior_transaction_execution;
mod catalogue_price_ui;
mod facility_accounting;
mod facility_economy_ui;
pub mod finance_table_ui;
mod guest_admission_payment;
mod guest_admission_ui;
mod keeper_refill_settlement;
mod monthly_finance_recording;
mod scenario_economy_command_application;
mod selected_object_sale_ui;
mod service_cancellation;
mod service_inventory_restocking;
mod service_progress_and_payment;
mod service_reservation;
mod zoo_cash_policy;
mod zoo_cash_ui;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::construction::FixedConstructionSet;

use self::{
    account_transaction_types::{TransactionCompleted, TransactionRejected, TransactionRequest},
    guest_admission_types::{AdmissionPrice, ZooAdmissionsOpen},
    monthly_finance_types::MonthlyFinanceHistory,
    scenario_economy_command_types::ScenarioEconomyCommand,
    service_types::{ServiceCompleted, ServiceRequestSequence},
    zoo_cash_types::ZooCash,
};

pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TransactionRequest>()
            .add_message::<TransactionCompleted>()
            .add_message::<TransactionRejected>()
            .add_message::<ScenarioEconomyCommand>()
            .init_resource::<ServiceRequestSequence>()
            .add_message::<ServiceCompleted>()
            .add_observer(service_cancellation::release_removed_service_reservation)
            .add_observer(service_cancellation::cancel_timed_reservations_when_facility_is_removed)
            .add_observer(service_cancellation::cancel_timed_reservation_when_guest_is_removed)
            .init_resource::<ZooCash>()
            .init_resource::<AdmissionPrice>()
            .init_resource::<MonthlyFinanceHistory>()
            .init_resource::<GeneralEconomyLedger>()
            .init_resource::<guest_admission_ui::AuthoredZooAdmissionPriceBands>()
            .init_resource::<ZooAdmissionsOpen>()
            .add_systems(
                Update,
                (
                    zoo_cash_policy::apply_selected_world_unlimited_cash_policy,
                    guest_admission_ui::initialize_zoo_admission_price_bands_from_loaded_world_definition,
                    guest_admission_ui::apply_zoo_admission_policy_changes_from_authored_ui_actions,
                    zoo_cash_ui::request_cash_grant_transactions_from_authored_ui_actions,
                    facility_economy_ui::route_selected_facility_economy_ui_actions,
                )
                    .chain().in_set(openzt2_zoo_state::economy::EconomyUiPolicyApplicationSet),
            )
            .add_systems(
                Update,
                (
                    zoo_cash_ui::project_zoo_cash_balance_into_authored_ui_text,
                    guest_admission_ui::project_zoo_admission_price_into_authored_ui_text,
                    guest_admission_ui::project_zoo_admissions_state_into_authored_toggle_selection
                        .in_set(openzt2_interface_state::ui::UiSet::DomainProjection)
                        .after(guest_admission_ui::apply_zoo_admission_policy_changes_from_authored_ui_actions),
                    guest_admission_ui::project_zoo_admissions_status_into_authored_ui_text
                        .after(guest_admission_ui::apply_zoo_admission_policy_changes_from_authored_ui_actions)
                        .after(openzt2_interface_state::ui::UiSet::Commands),
                    finance_table_ui::request_authored_finance_table_row_counts,
                    finance_table_ui::request_authored_finance_report_list_row_counts_and_bind_world_subjects,
                    finance_table_ui::project_monthly_finance_values_into_authored_table_rows,
                    (
                        finance_table_ui::bind_owning_balance_sheet_cells,
                        finance_table_ui::project_monthly_finance_values_into_native_balance_sheet_lists,
                    ).chain(),
                    facility_economy_ui::project_selected_facility_maintenance_schedule_options,
                    selected_object_sale_ui::project_selected_object_sale_confirmation,
                    catalogue_price_ui::project_authored_catalogue_entry_prices_into_dynamic_ui_rows
                        .after(
                            openzt2_interface_state::information::catalogue_types::CatalogueTypeListRowProjection,
                        ),
                )
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    authored_economy_fact_hydration::hydrate_authored_monthly_upkeep_for_new_world_objects,
                    authored_economy_fact_hydration::hydrate_authored_service_facility_facts_for_new_world_objects,
                    facility_accounting::initialize_new_service_facilities_with_operating_age_and_profit,
                    service_reservation::reserve_requested_facility_capacity_and_inventory_for_customers
                        .after(openzt2_zoo_state::guests::GuestViewingBegin),
                    service_progress_and_payment::advance_reserved_services_and_request_completed_service_payments,
                    behavior_transaction_execution::start_authored_behavior_transactions,
                    behavior_transaction_execution::execute_authored_behavior_transaction_steps,
                )
                    .chain()
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                (
                    guest_admission_payment::request_guest_admission_payment_after_entrance_reached,
                    account_transaction_settlement::stage_requested_account_transactions::<false>,
                    account_transaction_settlement::settle_requested_account_transactions::<false>,
                    guest_admission_payment::complete_guest_admission_after_successful_payment,
                    guest_admission_payment::reject_guest_admission_after_failed_payment,
                    facility_accounting::record_completed_transaction_results_for_attributed_facilities::<false>,
                    monthly_finance_recording::record_completed_transactions_and_guest_counts_in_monthly_finance_history::<false>,
                    service_inventory_restocking::restock_service_facility_inventories_after_zoo_days_advance,
                )
                    .chain()
                    .in_set(FixedGameSet::Economy),
            )
            .add_systems(
                FixedUpdate,
                (
                    account_transaction_settlement::stage_requested_account_transactions::<true>,
                    account_transaction_settlement::settle_requested_account_transactions::<true>,
                    (
                        facility_accounting::record_completed_transaction_results_for_attributed_facilities::<true>,
                        monthly_finance_recording::record_completed_transactions_and_guest_counts_in_monthly_finance_history::<true>,
                    ),
                    zoo_cash_ui::remove_resolved_ui_cash_grant_transaction_entities,
                ).chain().in_set(FixedConstructionSet::SettleEconomy),
            )
            .add_systems(
                FixedUpdate,
                (
                    service_progress_and_payment::complete_services_after_successful_payments,
                    service_progress_and_payment::cancel_services_after_rejected_payments,
                    behavior_transaction_execution::finish_settled_behavior_transactions,
                    behavior_transaction_execution::clear_interrupted_behavior_transaction_state,
                    service_cancellation::cancel_timed_service_for_departing_guests
                        .in_set(openzt2_zoo_state::economy::EconomyServiceCleanupSet)
                        .after(
                            openzt2_zoo_state::guests::GuestLifecycleAdvance,
                        ),
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup),
            )
            .add_systems(
                FixedUpdate,
                (
                    scenario_economy_command_application::apply_scenario_cash_and_admission_commands,
                    scenario_economy_command_application::remove_resolved_scenario_economy_command_transactions,
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup),
            )
            .add_systems(
                OnExit(GamePhase::InGame),
                (
                    zoo_cash_policy::clear_unlimited_cash_policy_after_leaving_game,
                    monthly_finance_recording::clear_finance_history_after_leaving_game,
                ),
            );
    }
}

#[cfg(test)]
mod account_transaction_settlement_tests;

#[cfg(test)]
mod guest_admission_payment_tests;

#[cfg(any(test, feature = "testing"))]
pub fn add_account_transaction_test_systems(app: &mut App) {
    app.add_message::<TransactionRequest>()
        .add_message::<TransactionCompleted>()
        .add_message::<TransactionRejected>()
        .init_resource::<ZooCash>()
        .add_systems(
            Update,
            (
                account_transaction_settlement::stage_requested_account_transactions::<false>,
                account_transaction_settlement::settle_requested_account_transactions::<false>,
                facility_accounting::record_completed_transaction_results_for_attributed_facilities::<false>,
                account_transaction_settlement::stage_requested_account_transactions::<true>,
                account_transaction_settlement::settle_requested_account_transactions::<true>,
                facility_accounting::record_completed_transaction_results_for_attributed_facilities::<true>,
            )
                .chain()
                .in_set(FixedGameSet::Economy),
        );
}
