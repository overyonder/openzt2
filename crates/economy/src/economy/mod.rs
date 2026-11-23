use openzt2_zoo_state::economy::account_transaction_types;
use openzt2_zoo_state::economy::authored_price_queries;
use openzt2_zoo_state::economy::authored_transaction_amount;
use openzt2_zoo_state::economy::behavior_transaction_state;
use openzt2_zoo_state::economy::currency_formatting;
use openzt2_zoo_state::economy::facility_economy_types;
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
mod experience_donation_payment;
mod facility_accounting;
mod facility_economy_ui;
mod finance_donation_reports;
pub mod finance_table_ui;
mod guest_admission_payment;
mod guest_admission_ui;
mod keeper_refill_settlement;
mod monthly_finance_recording;
mod saved_world_economy_trackers;
mod scenario_economy_command_application;
mod selected_object_sale_ui;
mod service_cancellation;
mod service_inventory_restocking;
mod service_progress_and_payment;
mod service_reservation;
mod zoo_cash_ui;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    BehaviorStepFixedSet, update_waiting_behavior_actors,
};
use openzt2_zoo_state::construction::FixedConstructionSet;

use self::{
    account_transaction_types::{TransactionCompleted, TransactionRejected, TransactionRequest},
    monthly_finance_types::MonthlyFinanceHistory,
    scenario_economy_command_types::ScenarioEconomyCommand,
    service_types::{ServiceCompleted, ServiceRequestSequence},
};

pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app.register_required_components::<
            WorldRoot,
            finance_donation_reports::FinanceDonationReports,
        >()
            .add_systems(
                Update,
                (
                    saved_world_economy_trackers::load_selected_world_economy,
                    saved_world_economy_trackers::materialize_selected_world_economy_trackers
                        .after(saved_world_economy_trackers::load_selected_world_economy),
                ).run_if(in_state(GamePhase::Loading).or_else(in_state(GamePhase::InGame))),
            )
            .add_message::<openzt2_zoo_state::transport_tours::transport_circuit_types::SetTransportCircuitFare>()
            .add_message::<TransactionRequest>()
            .add_message::<TransactionCompleted>()
            .add_message::<TransactionRejected>()
            .add_message::<ScenarioEconomyCommand>()

            .register_required_components::<WorldRoot, MonthlyFinanceHistory>()
            .register_required_components::<WorldRoot, ServiceRequestSequence>()
            .register_required_components::<facility_economy_types::Wallet, behavior_transaction_state::EconomyUsage>()
            .register_required_components::<facility_economy_types::ServiceFacility, behavior_transaction_state::EconomyUsage>()
            .add_systems(
                FixedUpdate,
                monthly_finance_recording::enter_published_zoo_finance_month
                    .after(FixedGameSet::Clock)
                    .before(account_transaction_settlement::settle_requested_account_transactions::<false>)
                    .before(FixedConstructionSet::SettleEconomy)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_message::<ServiceCompleted>()
            .add_observer(service_cancellation::release_removed_service_reservation)
            .add_observer(service_cancellation::cancel_timed_reservations_when_facility_is_removed)
            .add_observer(service_cancellation::cancel_timed_reservation_when_guest_is_removed)

            .add_systems(
                Update,
                (
                    guest_admission_ui::apply_zoo_admission_policy_changes_from_authored_ui_actions,
                    zoo_cash_ui::request_cash_grant_transactions_from_authored_ui_actions,
                    facility_economy_ui::route_selected_facility_economy_ui_actions,
                )
                    .in_set(openzt2_zoo_state::economy::EconomyUiPolicyApplicationSet),
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
                    finance_donation_reports::reduce_donations_for_finance_reports,
                    finance_donation_reports::project_finance_donation_rows
                        .after(finance_donation_reports::reduce_donations_for_finance_reports),
                    finance_table_ui::request_authored_finance_table_row_counts,
                    finance_table_ui::request_finance_balance_sheet_list_sizes,
                    finance_table_ui::bind_finance_report_rows,
                    finance_table_ui::project_finance_report_subjects
                        .after(finance_table_ui::bind_finance_report_rows),
                    finance_table_ui::project_monthly_finance_values_into_authored_table_rows,
                    finance_table_ui::bind_owning_balance_sheet_cells,
                    finance_table_ui::project_monthly_finance_values_into_native_balance_sheet_lists
                        .after(finance_table_ui::bind_owning_balance_sheet_cells),
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
                    facility_accounting::initialize_new_service_facilities_with_operating_age_and_profit
                        .after(authored_economy_fact_hydration::hydrate_authored_service_facility_facts_for_new_world_objects),
                    service_reservation::reserve_requested_facility_capacity_and_inventory_for_customers
                        .after(authored_economy_fact_hydration::hydrate_authored_service_facility_facts_for_new_world_objects)
                        .after(openzt2_zoo_state::guests::GuestViewingBegin),
                    service_progress_and_payment::advance_reserved_services_and_request_completed_service_payments
                        .after(service_reservation::reserve_requested_facility_capacity_and_inventory_for_customers),
                )
                    .in_set(FixedGameSet::Act),
            )
            .add_observer(behavior_transaction_execution::start_authored_behavior_transactions)
            .add_observer(experience_donation_payment::request_experience_donation_payments)
            .add_observer(
                behavior_transaction_execution::added_behavior_transaction
                    .pipe(behavior_transaction_execution::execute_authored_behavior_transaction_steps),
            )
            .add_systems(
                FixedUpdate,
                update_waiting_behavior_actors
                    .pipe(behavior_transaction_execution::execute_authored_behavior_transaction_steps)
                    .in_set(BehaviorStepFixedSet::Think),
            )
            .add_systems(
                FixedUpdate,
                (
                    keeper_refill_settlement::mark_staff_refill_cancellations_before_account_settlement
                        .before(account_transaction_settlement::settle_requested_account_transactions::<false>),
                    (
                        guest_admission_payment::request_guest_admission_payment_after_entrance_reached,
                        account_transaction_settlement::stage_requested_account_transactions::<false>,
                        account_transaction_settlement::settle_requested_account_transactions::<false>,
                        (
                            guest_admission_payment::complete_guest_admission_after_successful_payment,
                            guest_admission_payment::reject_guest_admission_after_failed_payment,
                            facility_accounting::record_completed_transaction_results_for_attributed_facilities::<false>,
                            experience_donation_payment::resume_show_donation_instructions_after_payment,
                            monthly_finance_recording::record_completed_transactions_and_guest_counts_in_monthly_finance_history::<false>,
                        ),
                    ).chain(),
                    service_inventory_restocking::restock_service_facility_inventories_after_zoo_days_advance,
                )
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
                    service_progress_and_payment::cancel_services_after_rejected_payments
                        .after(service_progress_and_payment::complete_services_after_successful_payments),
                    behavior_transaction_execution::finish_settled_behavior_transactions,
                    behavior_transaction_execution::clear_interrupted_behavior_transaction_state
                        .after(behavior_transaction_execution::finish_settled_behavior_transactions),
                    service_cancellation::cancel_timed_service_for_departing_guests
                        .after(service_progress_and_payment::cancel_services_after_rejected_payments)
                        .in_set(openzt2_zoo_state::economy::EconomyServiceCleanupSet)
                        .after(
                            openzt2_zoo_state::guests::GuestLifecycleAdvance,
                        ),
                )
                    .in_set(FixedGameSet::Cleanup),
            )
            .add_systems(
                FixedUpdate,
                (
                    scenario_economy_command_application::apply_scenario_cash_and_admission_commands,
                    scenario_economy_command_application::remove_resolved_scenario_economy_command_transactions,
                )
                    .in_set(FixedGameSet::Cleanup),
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

        .add_systems(
            Update,
            (
                (
                    account_transaction_settlement::stage_requested_account_transactions::<false>,
                    account_transaction_settlement::settle_requested_account_transactions::<false>,
                    account_transaction_settlement::stage_requested_account_transactions::<true>,
                    account_transaction_settlement::settle_requested_account_transactions::<true>,
                ).chain(),
                facility_accounting::record_completed_transaction_results_for_attributed_facilities::<false>
                    .after(account_transaction_settlement::settle_requested_account_transactions::<false>),
                facility_accounting::record_completed_transaction_results_for_attributed_facilities::<true>
                    .after(account_transaction_settlement::settle_requested_account_transactions::<true>)
                    .after(facility_accounting::record_completed_transaction_results_for_attributed_facilities::<false>),
            ).in_set(FixedGameSet::Economy),
        );
}
