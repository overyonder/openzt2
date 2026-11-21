//! Transactional construction, deletion, undo, and redo coordination.

mod catalogue_purchase_construction_tool_routing;
mod construction_confirmation;
pub(crate) mod construction_cursor_terrain_surface_tracking;
mod construction_delete_hover_target_resolution;
mod construction_edit_history_control_projection;
mod construction_edit_history_input_routing;
pub mod construction_history;
pub mod construction_lifecycle;
mod construction_payment_operation_types;
mod construction_payment_request_creation;
pub(crate) mod construction_tool_and_action_routing;
pub mod construction_transaction_preparation;
pub(crate) use construction_transaction_preparation::{
    capture_confirmed_preview, clear_confirmed_previews,
};
mod construction_ui_action_routing;
mod cursor_money_fragment_lifecycle;
mod cursor_money_presentation;
mod cursor_money_types;
pub mod deletion_transaction_preparation;
mod selected_biome_ui_projection;

use bevy::ecs::schedule::ApplyDeferred;
use bevy::prelude::*;
use construction_history::{
    record_history_application, retain_accepted_edits, WorldConstructionHistory,
};
use openzt2_zoo_state::construction::construction_edit_history_types::ApplyConstructionTransaction;
use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionTransactionApplied;
use openzt2_zoo_state::construction::construction_edit_history_types::RecordAppliedConstructionTransaction;
use openzt2_zoo_state::construction::construction_interaction_types::CancelConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::CommitConstruction;
use openzt2_zoo_state::construction::construction_interaction_types::DeleteEntity;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::SelectConstructionTool;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitFailed;
use openzt2_zoo_state::construction::construction_transaction_types::ConstructionCommitted;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareConstruction;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareDeletion;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

/// Installs the generic edit lifecycle. Domain plugins retain ownership of the
/// typed terrain, topology, and placement operations attached to transactions.
pub struct ConstructionPlugin;

/// Fixed-step player edits continue while the zoo simulation is paused. The
/// original pause control freezes simulation, not construction transactions.
use openzt2_zoo_state::construction::FixedConstructionSet;

/// Makes newly spawned transaction entities query-visible before any domain
/// reads the corresponding preparation request.
use openzt2_zoo_state::construction::FixedConstructionTransactionMaterializationSet;

/// Makes the domain-owned edit components inserted during preparation
/// query-visible before construction collects the matching prepared messages.
use openzt2_zoo_state::construction::FixedConstructionPreparedDomainEditMaterializationSet;

/// Orders the player-transaction steps after the simulation step. They run in
/// game whether or not the simulation is paused.
fn configure_fixed_construction_sets(app: &mut App) {
    app.configure_sets(
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
            .after(openzt2_world_state::application_schedule::FixedGameSet::Cleanup)
            .run_if(in_state(GamePhase::InGame)),
    );
}

impl Plugin for ConstructionPlugin {
    fn build(&self, app: &mut App) {
        app.register_required_components::<WorldRoot, WorldConstructionHistory>()
            .init_resource::<ConstructionTool>()
            .init_resource::<ConstructionPlacementPolicy>()
            .add_message::<SelectConstructionTool>()
            .add_message::<CommitConstruction>()
            .add_message::<CancelConstruction>()
            .add_message::<DeleteEntity>()
            .add_message::<PrepareConstruction>()
            .add_message::<PrepareDeletion>()
            .add_message::<RecordAppliedConstructionTransaction>()
            .add_message::<ApplyConstructionTransaction>()
            .add_message::<ConstructionTransactionApplied>()
            .add_message::<ConstructionCommitFailed>()
            .add_message::<ConstructionCommitted>();
        configure_fixed_construction_sets(app);
        app.add_observer(construction_confirmation::resolve_topology_confirmation)
            .add_systems(
                Update,
                construction_confirmation::capture_pending_construction_confirmation
                    .before(construction_ui_action_routing::route_construction_ui_actions)
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            );
        app
            .add_systems(
                Update,
                (
                    crate::construction::construction_ui_action_routing::route_construction_ui_actions,
                    crate::construction::construction_ui_action_routing::route_authored_construction_tool_activations,
                    crate::construction::catalogue_purchase_construction_tool_routing::route_construction_purchase_choices,
                    crate::construction::construction_tool_and_action_routing::select_construction_tool,
                    crate::construction::construction_edit_history_input_routing::request_undo_redo.run_if(
                        crate::construction::construction_edit_history_input_routing::no_pending_edits,
                    ),
                    crate::construction::construction_cursor_terrain_surface_tracking::ensure_construction_cursor,
                    crate::construction::construction_cursor_terrain_surface_tracking::update_construction_cursor.in_set(openzt2_zoo_state::construction::ConstructionCursorUpdateSet)
                        .run_if(
                            crate::construction::construction_tool_and_action_routing::construction_tool_active,
                        ),
                    crate::construction::construction_delete_hover_target_resolution::update_delete_hover_target,
                    crate::construction::construction_tool_and_action_routing::route_pointer_construction_action
                        .after(openzt2_zoo_state::placement::ObjectPlacementPreviewUpdateSet),
                    crate::construction::construction_tool_and_action_routing::route_construction_actions,
                )
                    .chain()
                    .in_set(GameSet::Intent),
            )
            .add_systems(
                Update,
                (
                    crate::construction::construction_tool_and_action_routing::cancel_preview.in_set(openzt2_zoo_state::construction::ConstructionPreviewCancellationSet),
                    crate::construction::construction_edit_history_control_projection::project_construction_undo_availability
                        .in_set(openzt2_interface_state::ui::UiSet::DomainProjection),
                    crate::construction::selected_biome_ui_projection::project_biome_panel_visibility,
                    crate::construction::selected_biome_ui_projection::project_selected_biome_name,
                )
                    .in_set(GameSet::Ui),
            )
            .add_systems(
                Update,
                (
                    crate::construction::cursor_money_fragment_lifecycle::identify_authored_cursor_money_container,
                    crate::construction::cursor_money_fragment_lifecycle::reconcile_cursor_money_fragments,
                    crate::construction::cursor_money_presentation::project_cursor_money_text_position_and_animation,
                )
                    .chain()
                    .in_set(openzt2_interface_state::ui::UiSet::DomainProjection),
            )
            .add_systems(
                Update,
                crate::construction::cursor_money_fragment_lifecycle::remove_completed_cursor_money_spend_fragments
                    .in_set(openzt2_interface_state::ui::UiSet::Notifications),
            )
            .add_systems(
                FixedUpdate,
                (
                    crate::construction::construction_transaction_preparation::begin_construction_transaction_from_valid_preview,
                    crate::construction::deletion_transaction_preparation::begin_deletion_transaction_preparation,
                    ApplyDeferred.in_set(FixedConstructionTransactionMaterializationSet),
                    ApplyDeferred.in_set(FixedConstructionPreparedDomainEditMaterializationSet),
                )
                    .chain()
                    .in_set(FixedConstructionSet::Prepare),
            )
            .add_systems(
                PostUpdate,
                capture_confirmed_preview::<openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview>,
            )
            .add_systems(
                FixedUpdate,
                clear_confirmed_previews::<openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview>
                    .after(FixedConstructionSet::Prepare)
                    .before(FixedConstructionSet::Apply),
            )
            .add_systems(
                FixedUpdate,
                (retain_accepted_edits, record_history_application).in_set(FixedConstructionSet::Cleanup),
            )
            .add_systems(OnExit(GamePhase::InGame), reset_construction_state);
        construction_lifecycle::install(app);
    }
}

fn reset_construction_state(
    mut tool: ResMut<ConstructionTool>,
    mut policy: ResMut<ConstructionPlacementPolicy>,
) {
    *tool = default();
    *policy = default();
}
