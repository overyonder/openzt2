//! Animal show stages, authored schedules, editor state, and platform upgrades.

pub use running_show_schedule::{
    ActiveShowTrickOf, ActiveShowTricks, ShowColumnWait, ShowRunSlotOrder,
};
pub use scheduled_trick_execution::{ScheduledTrickToken, ShowTrickTokens, TrickTokenForShow};
pub use show_scoring::ShowScoreSamples;

mod authored_show_rewards;
mod running_show_schedule;
mod scheduled_trick_execution;
mod show_audience_lifecycle;
mod show_audience_types;
mod show_run_presentation;
pub use show_run_presentation::{ShowAudienceFeedback, ShowMasterOfCeremoniesCue};
mod show_donation_summary_presentation;
mod show_editor_interaction_types;
mod show_editor_presentation_lifecycle;
mod show_mixer_presentation;
mod show_mixer_slots;
pub use show_mixer_slots::{
    EditShowMixerSlot, ShowMixerSlotChoice, ShowMixerTimeSlot, ShowMixerWait,
};
mod show_editor_ui_action_routing;
pub(crate) mod show_event_lifecycle;
mod show_scoring;
mod show_trick_props;
mod show_viewing_locations;
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
pub mod show_timing_operations;

use bevy::prelude::*;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorStepFixedSet;
use openzt2_zoo_state::construction::FixedConstructionSet;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::shows::running_show_types::{
    BeginScheduledShowTrick, ScheduledShowTrickScored, ShowAudienceReaction, ShowRunFinished,
    WorldTrickPopularity,
};
use show_platform_upgrade_types::{
    CommitShowPlatformUpgradeEdit, RestoreShowPlatformUpgrade, ShowPlatformUpgradeEditAcknowledged,
};

pub struct ShowsPlugin;

impl Plugin for ShowsPlugin {
    fn build(&self, app: &mut App) {
        use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
            activated_behavior_actor, update_waiting_behavior_actors,
        };
        app.add_observer(activated_behavior_actor::<openzt2_zoo_state::behavior_task_execution_types::running_actions::RunBehaviorWatchShow>.pipe(show_audience_lifecycle::bind_ready_show_watch_actions));

        app.add_observer(show_mixer_presentation::clear_closed_mixer_menu)
            .add_systems(Startup, show_mixer_presentation::load_show_mixer_fragments)
            .add_systems(
                Update,
                show_mixer_presentation::route_live_show_mixer_controls
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    show_mixer_presentation::project_live_show_mixer,
                    show_mixer_presentation::project_authored_show_score_pips,
                    show_mixer_presentation::bind_projected_show_mixer_fields
                        .after(show_mixer_presentation::project_live_show_mixer),
                    show_mixer_presentation::attach_show_mixer_tricks_to_authored_row
                        .after(show_mixer_presentation::project_live_show_mixer),
                )
                    .in_set(GameSet::Ui)
                    .run_if(in_state(GamePhase::InGame)),
            );
        app.add_systems(Update, show_mixer_slots::apply_show_mixer_slot_edits
            .after(show_mixer_presentation::route_live_show_mixer_controls)
            .in_set(GameSet::Intent)
            .after(show_schedule_ui_action_routing::route_authored_show_schedule_actions_into_rows_and_editor_selection)
            .after(show_editor_ui_action_routing::route_authored_show_editor_actions_into_selection_and_immersive_mode_state)
            .run_if(in_state(GamePhase::InGame)));
        app.register_required_components::<openzt2_world_state::world_spawn::world_membership_types::WorldRoot, WorldTrickPopularity>()
            .init_asset::<authored_show_rewards::AuthoredShowRewards>()
            .init_asset_loader::<authored_show_rewards::AuthoredShowRewardsLoader>()
            .add_systems(Startup, authored_show_rewards::load_authored_show_rewards)
            .add_systems(FixedUpdate, show_scoring::grow_trick_popularity
                .in_set(FixedGameSet::Clock)
                .run_if(in_state(GamePhase::InGame)))
            .add_message::<BeginScheduledShowTrick>()
            .add_message::<ScheduledShowTrickScored>()
            .add_message::<ShowRunFinished>()
            .add_message::<running_show_schedule::ShowRunCancelled>()
            .add_message::<running_show_schedule::ShowPhaseStarted>()
            .add_message::<ShowAudienceReaction>()
            .add_message::<EditShowMixerSlot>()
            .add_message::<ShowAudienceFeedback>()
            .add_message::<ShowMasterOfCeremoniesCue>()
            .add_message::<openzt2_zoo_state::guests::guest_simulation_types::GuestNeedAdjustmentDue>()
            .add_message::<openzt2_zoo_state::habitat::habitat_cell_ownership_changes::HabitatCellOwnershipChanged>()
            .add_observer(show_audience_lifecycle::clear_completed_show_watch_action)
            .add_observer(show_audience_lifecycle::discard_experience_when_guest_stops_watching)
            .add_observer(show_event_lifecycle::discard_delivered_event_when_its_window_retires)
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
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                openzt2_zoo_state::shows::show_stage_habitat_projection::project_show_stage_habitats
                    .after(show_stage_identification_and_hydration::hydrate_show_stages_from_active_world_definitions)
                    .after(openzt2_zoo_state::habitat::habitat_types::HabitatTopologyPublication)
                    .before(FixedConstructionSet::Prepare)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                show_audience_lifecycle::reconcile_guest_show_needs
                    .in_set(FixedGameSet::Think)
                    .after(openzt2_zoo_state::guests::GuestDestinationSelection)
                    .before(BehaviorStepFixedSet::Think)
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
                            .after(running_show_schedule::hydrate_restored_show_columns)
                            .after(show_event_lifecycle::dispatch_released_animal_show_events),
                        running_show_schedule::start_next_stage_show_or_break
                            .after(running_show_schedule::initialize_show_schedule_cursors),
                    ).chain(),
                    show_audience_lifecycle::retire_interrupted_show_watch_actions
                        .after(openzt2_zoo_state::guests::GuestLifecycleAdvance),
                    running_show_schedule::initialize_show_schedule_cursors,
                    running_show_schedule::hydrate_restored_show_columns,
                    show_event_lifecycle::schedule_authored_animal_show_events,
                    show_scoring::apply_authored_trick_outcome_scores
                        .after(show_event_lifecycle::schedule_authored_animal_show_events),
                    show_trick_props::load_scheduled_prop_prefabs
                        .after(running_show_schedule::start_next_stage_show_or_break),
                    show_scoring::predict_performance_scores
                        .after(running_show_schedule::start_next_stage_show_or_break),
                    scheduled_trick_execution::start_scheduled_tricks_with_authored_outcome_tokens
                        .after(show_trick_props::load_scheduled_prop_prefabs),
                    show_viewing_locations::refresh_stage_viewing_locations
                        .after(running_show_schedule::start_next_stage_show_or_break),
                    show_audience_lifecycle::invite_guests_to_starting_shows
                        .after(running_show_schedule::start_next_stage_show_or_break),
                    show_scoring::smooth_finished_show_scores
                        .after(running_show_schedule::advance_running_stage_shows),
                    show_event_lifecycle::dispatch_released_animal_show_events
                        .after(show_event_lifecycle::update_authored_show_event_windows)
                        .after(show_scoring::apply_authored_trick_outcome_scores)
                        .after(show_audience_lifecycle::retire_interrupted_show_watch_actions),
                    show_event_lifecycle::update_authored_show_event_windows
                        .after(show_event_lifecycle::schedule_authored_animal_show_events),
                    show_run_presentation::dispatch_run_presentation
                        .after(running_show_schedule::start_next_stage_show_or_break),
                    show_audience_lifecycle::complete_watch_actions_after_show
                        .after(running_show_schedule::advance_running_stage_shows),
                )
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                update_waiting_behavior_actors.pipe(show_audience_lifecycle::bind_ready_show_watch_actions)
                    .in_set(BehaviorStepFixedSet::Think),
            );
    }
}
