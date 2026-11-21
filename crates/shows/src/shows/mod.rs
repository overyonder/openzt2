//! Animal show stages, authored schedules, editor state, and platform upgrades.

mod running_show_schedule;
mod show_audience_lifecycle;
mod show_audience_types;
mod show_donation_summary_presentation;
mod show_editor_interaction_types;
mod show_editor_presentation_lifecycle;
mod show_editor_ui_action_routing;
pub(crate) mod show_event_lifecycle;
use openzt2_zoo_state::shows::show_event_types;
mod show_platform_upgrade_lifecycle;
mod show_platform_upgrade_presentation;
use openzt2_zoo_state::shows::show_platform_upgrade_types;
mod show_platform_upgrade_ui_action_routing;
use openzt2_zoo_state::shows::show_schedule_types;
mod show_schedule_retirement;
mod show_schedule_ui_action_routing;
mod show_schedule_ui_mutation_operations;
mod show_stage_identification_and_hydration;
use openzt2_zoo_state::shows::show_stage_types;
mod show_timing_operations;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::shows::running_show_types::{
    BeginScheduledShowTrick, ScheduledShowTrickScored, ShowAudienceReaction, ShowRunFinished,
};
use show_platform_upgrade_types::{
    CommitShowPlatformUpgradeEdit, RestoreShowPlatformUpgrade, ShowPlatformUpgradeEditAcknowledged,
};

pub struct ShowsPlugin;

impl Plugin for ShowsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BeginScheduledShowTrick>()
            .add_message::<ScheduledShowTrickScored>()
            .add_message::<ShowRunFinished>()
            .add_message::<ShowAudienceReaction>()
            .add_observer(show_audience_lifecycle::clear_completed_show_watch_action)
            .add_message::<CommitShowPlatformUpgradeEdit>()
            .add_message::<ShowPlatformUpgradeEditAcknowledged>()
            .add_message::<RestoreShowPlatformUpgrade>()
            .add_observer(show_schedule_retirement::retire_schedule_and_editor_links_when_stage_is_removed)
            .add_observer(show_schedule_retirement::remove_trick_slots_when_animal_is_removed)
            .add_observer(show_schedule_retirement::clear_editor_after_schedule_selection_is_removed)
            .add_observer(show_schedule_retirement::clear_editing_after_edited_show_is_removed)
            .add_systems(
                Update,
                (
                    show_platform_upgrade_lifecycle::queue_show_platform_upgrade_restores,
                    show_platform_upgrade_lifecycle::restore_show_platform_upgrades
                        .run_if(
                            any_match_filter::<Or<(
                                Added<show_platform_upgrade_types::PendingShowPlatformUpgradeRestore>,
                                Added<show_stage_types::ShowStage>,
                                Added<openzt2_world_state::world_spawn::prefab_authored_attachment_identifier::PrefabAuthoredAttachmentIdentifier>,
                            )>>.or_else(resource_changed::<Assets<openzt2_game_assets::scene_prefab::ScenePrefabAsset>>)
                            .or_else(resource_changed::<Assets<bevy::gltf::Gltf>>)
                            .or_else(resource_changed::<openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions>),
                        ),
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
                show_stage_identification_and_hydration::hydrate_show_stages_from_active_world_definitions
                    .run_if(show_stage_identification_and_hydration::show_stage_definitions_are_available)
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                show_platform_upgrade_lifecycle::apply_show_platform_upgrade_edits
                    .in_set(openzt2_zoo_state::construction::FixedConstructionSet::Apply)
                    .after(openzt2_zoo_state::construction::ConstructionDomainApplicationDispatchSet)
                    .after(openzt2_zoo_state::construction::ConstructionFailureRollbackDispatchSet)
                    .before(openzt2_zoo_state::construction::ConstructionDomainAcknowledgementCollectionSet),
            )
            .add_systems(
                FixedUpdate,
                (
                    (
                        running_show_schedule::advance_running_stage_shows
                            .after(show_event_lifecycle::dispatch_released_animal_show_events),
                        running_show_schedule::start_next_stage_show_or_break
                            .after(running_show_schedule::initialize_show_schedule_cursors),
                    ).chain(),
                    (
                        show_audience_lifecycle::retire_interrupted_show_watch_actions,
                        show_audience_lifecycle::bind_ready_show_watch_actions,
                    ).chain().after(openzt2_zoo_state::guests::GuestLifecycleAdvance),
                    running_show_schedule::initialize_show_schedule_cursors,
                    show_event_lifecycle::schedule_authored_animal_show_events,
                    running_show_schedule::apply_authored_trick_outcome_scores,
                    show_event_lifecycle::dispatch_released_animal_show_events
                        .after(show_event_lifecycle::schedule_authored_animal_show_events)
                        .after(running_show_schedule::apply_authored_trick_outcome_scores)
                        .after(show_audience_lifecycle::bind_ready_show_watch_actions),
                    show_audience_lifecycle::complete_watch_actions_after_show
                        .after(running_show_schedule::advance_running_stage_shows),
                )
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}
