//! Animal show stages, authored schedules, editor state, and platform upgrades.

mod cleanup;
mod show_donation_summary_presentation;
mod show_editor_interaction_types;
mod show_editor_presentation_lifecycle;
mod show_editor_ui_action_routing;
mod show_platform_upgrade_lifecycle;
mod show_platform_upgrade_presentation;
pub(crate) mod show_platform_upgrade_types;
mod show_platform_upgrade_ui_action_routing;
pub(crate) mod show_schedule_types;
mod show_schedule_ui_action_routing;
mod show_schedule_ui_mutation_operations;
mod show_scheduler_link_cleanup;
mod show_stage_identification_and_hydration;
pub(crate) mod show_stage_types;
mod show_timing_operations;

use bevy::prelude::*;

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::{FixedGameSet, GameSet};
use show_platform_upgrade_types::{
    CommitShowPlatformUpgradeEdit, RestoreShowPlatformUpgrade, ShowPlatformUpgradeEditAcknowledged,
};

pub struct ShowsPlugin;

impl Plugin for ShowsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<CommitShowPlatformUpgradeEdit>()
            .add_message::<ShowPlatformUpgradeEditAcknowledged>()
            .add_message::<RestoreShowPlatformUpgrade>()
            .add_systems(
                Update,
                (
                    show_platform_upgrade_lifecycle::queue_show_platform_upgrade_restores,
                    show_platform_upgrade_lifecycle::restore_show_platform_upgrades,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (
                    show_editor_ui_action_routing::
                        route_authored_show_editor_actions_into_selection_and_immersive_mode_state,
                    show_schedule_ui_action_routing::
                        route_authored_show_schedule_actions_into_rows_and_editor_selection,
                    show_platform_upgrade_ui_action_routing::
                        route_authored_show_platform_upgrade_actions_into_selection_edits_and_deletion,
                    show_editor_presentation_lifecycle::
                        show_authored_show_editor_for_entered_show_edit_mode,
                    show_editor_presentation_lifecycle::
                        hide_authored_show_editor_after_immersive_mode_exit,
                    show_platform_upgrade_presentation::
                        project_show_platform_upgrade_selection_and_purchase_availability_into_authored_controls,
                )
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                show_donation_summary_presentation::
                    project_selected_or_all_show_donation_summary_into_authored_controls
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    show_stage_identification_and_hydration::
                        identify_and_hydrate_spawned_show_stage_world_objects,
                    show_platform_upgrade_lifecycle::apply_show_platform_upgrade_edits,
                )
                    .chain()
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    show_scheduler_link_cleanup::
                        remove_show_schedule_entries_and_editor_links_with_missing_owners,
                    cleanup::remove_departed_animals_from_scheduled_show_performances,
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}
