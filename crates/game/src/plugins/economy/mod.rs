mod account_transaction_settlement;
pub(crate) mod account_transaction_types;
pub(crate) mod authored_economy_fact_hydration;
mod authored_transaction_amount;
mod behavior_transaction_execution;
pub(crate) mod behavior_transaction_state;
mod catalogue_price_ui;
mod facility_accounting;
pub(crate) mod facility_economy_types;
mod facility_economy_ui;
pub(crate) mod finance_table_ui;
mod guest_admission_payment;
pub(crate) mod guest_admission_types;
mod guest_admission_ui;
pub(crate) mod money_types;
mod monthly_finance_recording;
pub(crate) mod monthly_finance_types;
pub(crate) mod object_sale_refund;
pub(crate) mod price_effect;
mod scenario_economy_command_application;
pub(crate) mod scenario_economy_command_types;
mod selected_object_sale_ui;
pub(crate) mod service_cancellation;
mod service_capacity_and_inventory_release;
mod service_inventory_restocking;
mod service_progress_and_payment;
mod service_reservation;
pub(crate) mod service_types;
mod zoo_cash_policy;
pub(crate) mod zoo_cash_types;
mod zoo_cash_ui;

use bevy::prelude::*;

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::{FixedGameSet, GameSet};
use crate::plugins::construction::FixedConstructionSet;

use self::{
    account_transaction_types::{TransactionCompleted, TransactionRejected, TransactionRequest},
    guest_admission_types::{AdmissionPrice, ZooAdmissionsOpen},
    monthly_finance_types::MonthlyFinanceHistory,
    scenario_economy_command_types::ScenarioEconomyCommand,
    service_types::{ServiceCompleted, ServiceRequest},
    zoo_cash_types::ZooCash,
};

pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TransactionRequest>()
            .add_message::<TransactionCompleted>()
            .add_message::<TransactionRejected>()
            .add_message::<ScenarioEconomyCommand>()
            .add_message::<ServiceRequest>()
            .add_message::<ServiceCompleted>()
            .init_resource::<ZooCash>()
            .init_resource::<AdmissionPrice>()
            .init_resource::<MonthlyFinanceHistory>()
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
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    zoo_cash_ui::project_zoo_cash_balance_into_authored_ui_text,
                    guest_admission_ui::project_zoo_admission_price_into_authored_ui_text,
                    finance_table_ui::request_authored_finance_table_row_counts,
                    finance_table_ui::request_authored_finance_report_list_row_counts_and_bind_world_subjects,
                    finance_table_ui::project_monthly_finance_values_into_authored_table_rows,
                    finance_table_ui::project_monthly_finance_values_into_native_balance_sheet_lists,
                    facility_economy_ui::project_selected_facility_maintenance_schedule_options,
                    selected_object_sale_ui::project_selected_object_sale_confirmation,
                    catalogue_price_ui::project_authored_catalogue_entry_prices_into_dynamic_ui_rows
                        .after(
                            crate::plugins::information::catalogue::catalogue_type_list_row_projection::project_catalogue_entries_to_authored_type_list_rows,
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
                        .after(crate::plugins::guests::guest_destination_and_viewing_execution::begin_viewing),
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
                    account_transaction_settlement::settle_requested_account_transactions,
                    guest_admission_payment::complete_guest_admission_after_successful_payment,
                    guest_admission_payment::reject_guest_admission_after_failed_payment,
                    facility_accounting::record_completed_transaction_results_for_attributed_facilities,
                    monthly_finance_recording::record_completed_transactions_and_guest_counts_in_monthly_finance_history,
                    service_inventory_restocking::restock_service_facility_inventories_after_zoo_days_advance,
                )
                    .chain()
                    .in_set(FixedGameSet::Economy),
            )
            .add_systems(
                FixedUpdate,
                account_transaction_settlement::settle_requested_account_transactions
                    .in_set(FixedConstructionSet::SettleEconomy)
                    .run_if(crate::plugins::simulation_time::simulation_control_application::simulation_is_paused),
            )
            .add_systems(
                FixedUpdate,
                (
                    service_progress_and_payment::complete_services_after_successful_payments,
                    service_progress_and_payment::cancel_services_after_rejected_payments,
                    behavior_transaction_execution::finish_settled_behavior_transactions,
                    behavior_transaction_execution::clear_interrupted_behavior_transaction_state,
                    service_cancellation::cancel_services_for_departed_guests_and_missing_facilities
                        .after(
                            crate::plugins::guests::guest_lifecycle_and_memory_execution::advance_guest_lifecycle,
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
                    zoo_cash_ui::remove_resolved_ui_cash_grant_transaction_entities,
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup),
            )
            .add_systems(
                OnExit(GamePhase::InGame),
                zoo_cash_policy::clear_unlimited_cash_policy_after_leaving_game,
            );
    }
}

#[cfg(test)]
mod account_transaction_settlement_tests;

#[cfg(test)]
mod guest_admission_payment_tests;
