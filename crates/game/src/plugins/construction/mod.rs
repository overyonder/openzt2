//! Transactional construction, deletion, undo, and redo coordination.

mod catalogue_purchase_construction_tool_routing;
mod construction_application_failure_recovery;
mod construction_compensation_request;
pub(crate) mod construction_cursor_terrain_surface_tracking;
mod construction_delete_hover_target_resolution;
pub(crate) mod construction_domain_acknowledgement_collection;
pub(crate) mod construction_domain_application_dispatch;
pub(crate) mod construction_domain_preparation_collection;
mod construction_economy_result_completion;
mod construction_edit_history_application_request;
mod construction_edit_history_input_routing;
mod construction_edit_history_operations;
mod construction_edit_history_settlement;
pub(crate) mod construction_edit_history_types;
pub(crate) mod construction_failure_rollback_dispatch;
mod construction_initial_economy_request;
pub(crate) mod construction_interaction_types;
mod construction_payment_operation_types;
mod construction_payment_request_creation;
pub(crate) mod construction_tool_and_action_routing;
pub(crate) mod construction_tool_and_placement_policy_types;
pub(crate) mod construction_transaction_preparation;
pub(crate) mod construction_transaction_types;
mod construction_ui_action_routing;
mod cursor_money_fragment_lifecycle;
mod cursor_money_presentation;
mod cursor_money_types;
pub(crate) mod deletion_transaction_preparation;
pub(crate) mod selected_biome_ui_projection;

use bevy::{ecs::schedule::ApplyDeferred, prelude::*};
use construction_edit_history_settlement::{
    advance_construction_edit_history_after_accepted_application,
    record_applied_construction_transactions_in_history,
};
use construction_edit_history_types::{
    ApplyConstructionTransaction, ConstructionEditHistory, ConstructionTransactionApplied,
    RecordAppliedConstructionTransaction,
};
use construction_interaction_types::{CancelConstruction, CommitConstruction, DeleteEntity};
use construction_tool_and_placement_policy_types::{
    ConstructionPlacementPolicy, ConstructionTool, SelectConstructionTool,
};
use construction_transaction_types::{
    ConstructionCommitFailed, ConstructionCommitted, EditApplicationAuthorized,
    PrepareConstruction, PrepareDeletion,
};

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::GameSet;

/// Installs the generic edit lifecycle. Domain plugins retain ownership of the
/// typed terrain, topology, and placement operations attached to transactions.
pub struct ConstructionPlugin;

/// Fixed-step player edits continue while the zoo simulation is paused. The
/// original pause control freezes simulation, not construction transactions.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FixedConstructionSet {
    Prepare,
    RequestEconomy,
    SettleEconomy,
    CompleteEconomy,
    Apply,
    Cleanup,
}

/// Makes newly spawned transaction entities query-visible before any domain
/// reads the corresponding preparation request.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FixedConstructionTransactionMaterializationSet;

/// Makes the domain-owned edit components inserted during preparation
/// query-visible before construction collects the matching prepared messages.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FixedConstructionPreparedDomainEditMaterializationSet;

impl Plugin for ConstructionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConstructionTool>()
            .init_resource::<ConstructionEditHistory>()
            .init_resource::<ConstructionPlacementPolicy>()
            .add_message::<SelectConstructionTool>()
            .add_message::<CommitConstruction>()
            .add_message::<CancelConstruction>()
            .add_message::<DeleteEntity>()
            .add_message::<PrepareConstruction>()
            .add_message::<PrepareDeletion>()
            .add_message::<EditApplicationAuthorized>()
            .add_message::<RecordAppliedConstructionTransaction>()
            .add_message::<ApplyConstructionTransaction>()
            .add_message::<ConstructionTransactionApplied>()
            .add_message::<ConstructionCommitFailed>()
            .add_message::<ConstructionCommitted>()
            .configure_sets(
                FixedUpdate,
                (
                    FixedConstructionSet::Prepare,
                    FixedConstructionSet::RequestEconomy,
                    FixedConstructionSet::SettleEconomy,
                    FixedConstructionSet::CompleteEconomy,
                    FixedConstructionSet::Apply,
                    FixedConstructionSet::Cleanup,
                )
                    .chain()
                    .after(crate::application_schedule::FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    construction_ui_action_routing::route_construction_ui_actions,
                    catalogue_purchase_construction_tool_routing::route_construction_purchase_choices,
                    construction_tool_and_action_routing::select_construction_tool,
                    construction_edit_history_input_routing::request_undo_redo.run_if(
                        construction_edit_history_input_routing::no_pending_edits,
                    ),
                    construction_cursor_terrain_surface_tracking::ensure_construction_cursor,
                    construction_cursor_terrain_surface_tracking::update_construction_cursor
                        .run_if(
                            construction_tool_and_action_routing::construction_tool_active,
                        ),
                    construction_delete_hover_target_resolution::update_delete_hover_target,
                    construction_tool_and_action_routing::route_pointer_construction_action,
                    construction_tool_and_action_routing::route_construction_actions,
                )
                    .chain()
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                (
                    construction_tool_and_action_routing::cancel_preview,
                    selected_biome_ui_projection::project_biome_panel_visibility,
                    selected_biome_ui_projection::project_selected_biome_name,
                )
                    .in_set(GameSet::Ui),
            )
            .add_systems(
                Update,
                (
                    cursor_money_fragment_lifecycle::identify_authored_cursor_money_container,
                    cursor_money_fragment_lifecycle::reconcile_cursor_money_fragments,
                    cursor_money_presentation::project_cursor_money_text_position_and_animation,
                )
                    .chain()
                    .in_set(crate::plugins::ui::UiSet::DomainProjection),
            )
            .add_systems(
                Update,
                cursor_money_fragment_lifecycle::remove_completed_cursor_money_spend_fragments
                    .in_set(crate::plugins::ui::UiSet::Notifications),
            )
            .add_systems(
                FixedUpdate,
                (
                    construction_transaction_preparation::begin_construction_transaction_from_valid_preview,
                    deletion_transaction_preparation::begin_deletion_transaction_preparation,
                    ApplyDeferred.in_set(FixedConstructionTransactionMaterializationSet),
                    ApplyDeferred.in_set(FixedConstructionPreparedDomainEditMaterializationSet),
                    construction_domain_preparation_collection::collect_construction_domain_preparation_results,
                )
                    .chain()
                    .in_set(FixedConstructionSet::Prepare),
            )
            .add_systems(
                FixedUpdate,
                construction_initial_economy_request::request_initial_construction_payment_or_authorize_zero_cost_application
                    .in_set(FixedConstructionSet::RequestEconomy),
            )
            .add_systems(
                FixedUpdate,
                (
                    construction_economy_result_completion::complete_construction_payment_results,
                    construction_edit_history_application_request::begin_requested_construction_edit_history_application,
                    construction_compensation_request::request_construction_undo_or_failure_compensation,
                )
                    .chain()
                    .in_set(FixedConstructionSet::CompleteEconomy),
            )
            .add_systems(
                FixedUpdate,
                (
                    construction_domain_application_dispatch::dispatch_authorized_construction_application_to_participating_domains,
                    construction_failure_rollback_dispatch::dispatch_construction_failure_rollback_to_changed_domains_once,
                )
                    .chain()
                    .in_set(FixedConstructionSet::Apply),
            )
            .add_systems(
                FixedUpdate,
                (
                    construction_domain_acknowledgement_collection::collect_construction_domain_application_acknowledgements,
                    record_applied_construction_transactions_in_history,
                    advance_construction_edit_history_after_accepted_application,
                )
                    .chain()
                    .in_set(FixedConstructionSet::Cleanup),
            )
            .add_systems(OnExit(GamePhase::InGame), reset_construction_state);
    }
}

fn reset_construction_state(
    mut tool: ResMut<ConstructionTool>,
    mut history: ResMut<ConstructionEditHistory>,
    mut policy: ResMut<ConstructionPlacementPolicy>,
) {
    *tool = default();
    *history = default();
    *policy = default();
}

#[cfg(test)]
mod construction_domain_acknowledgement_tests;
#[cfg(test)]
mod construction_domain_preparation_tests;
