//! Visitor lifecycle represented directly by small ECS facts.

use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    BehaviorStep, BehaviorStepFixedSet, BehaviorStepSet,
};
use openzt2_zoo_state::guests::GuestDestinationSelection;
use openzt2_zoo_state::guests::GuestLifecycleAdvance;
use openzt2_zoo_state::guests::GuestReactionApplication;
use openzt2_zoo_state::guests::GuestViewingBegin;

mod guest_animation_owner_projection;
mod guest_arrival_calculations;
pub(crate) mod guest_arrival_execution;
mod guest_behavior_fact_execution;
mod guest_definition_queries;
pub(crate) mod guest_destination_and_viewing_execution;
mod guest_favourite_animal_trend_execution;
pub(crate) mod guest_lifecycle_and_memory_execution;
pub(crate) mod guest_need_execution;
mod guest_self_task_selection;
use openzt2_zoo_state::guests::guest_simulation_calculations;
use openzt2_zoo_state::guests::guest_simulation_types;
pub(crate) mod guest_survey_execution;
mod guest_survey_need_sampling;
mod guest_survey_scoring;
mod guest_target_eligibility;
mod guest_task_need_facts;
mod guest_task_progress_diagnostics;
mod guest_task_target_loss;
mod guest_view_animals_execution;
mod guest_view_event_meter_application;
mod guest_viewing_lifecycle;
pub(crate) mod view_event_execution;

use bevy::prelude::*;
use guest_arrival_execution::{
    initialize_guest_arrivals, initialize_spawned_guests, report_changed_guest_arrival_failures,
    spawn_arriving_guests,
};
use guest_destination_and_viewing_execution::hydrate_viewing_opportunities;
use guest_destination_and_viewing_execution::{
    advance_viewing, begin_viewing, cancel_failed_guest_destinations, choose_guest_destination,
    react_to_visible_litter,
};
use guest_lifecycle_and_memory_execution::{
    advance_guest_lifecycle, age_guest_memories, despawn_departed_guests,
};
use guest_need_execution::{apply_guest_satisfaction_adjustments, decay_guest_needs};
use guest_survey_execution::{
    adjust_guest_survey_data, advance_guest_survey_day, initialize_guest_survey,
    route_guest_survey_triggers,
};

use guest_simulation_types::*;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::guests::guest_survey_types::GuestSurvey;

pub struct GuestsPlugin;

impl Plugin for GuestsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<view_event_execution::BehaviorViewEventEmitted>()
            .add_message::<GuestReaction>()
            .add_message::<view_event_execution::AppliedAnimalView>()
            .add_message::<AdjustGuestSatisfaction>()
            .add_message::<AdjustGuestSurveyData>()
            .add_message::<openzt2_zoo_state::animal_behavior::behavior_trigger_types::BehaviorSubjectTrigger>()
            .add_message::<GuestArrived>()
            .add_message::<GuestReachedEntrance>()
            .add_message::<GuestDeparted>()
            .add_message::<GuestArrivalFailed>()
            .add_message::<RequestGuestArrivals>()
            .add_systems(
                Update,
                (
                    initialize_guest_arrivals,
                    guest_favourite_animal_trend_execution::initialize_loaded_world_guest_favourite_animal_trends.after(initialize_guest_arrivals),
                    initialize_guest_survey,
                    report_changed_guest_arrival_failures,
                    apply_guest_satisfaction_adjustments,
                )
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    guest_animation_owner_projection::request_authored_animation_sets_for_resolved_guest_models,
                    guest_animation_owner_projection::attach_loaded_animation_sets_to_guest_models,
                    guest_animation_owner_projection::project_guest_animation_owners,
                )
                    .in_set(GameSet::Presentation)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                BehaviorStep,
                (
                    guest_behavior_fact_execution::apply_authored_guest_behavior_fact_modifications
                        .after(openzt2_zoo_state::maintenance::maintenance_types::FacilityWasteBehaviorModification),
                    view_event_execution::emit_authored_view_events,
                    guest_view_animals_execution::watch_animals_for_current_view_actions,
                ).in_set(BehaviorStepSet::Actions),
            )
            .add_systems(
                FixedUpdate,
                (
                    initialize_spawned_guests,
                    guest_view_event_meter_application::initialize_guest_species_view_counts,
                    guest_task_target_loss::fail_guest_tasks_whose_target_is_gone,
                    hydrate_viewing_opportunities,
                    guest_destination_and_viewing_execution::project_authored_viewing_ground_slots
                        .after(hydrate_viewing_opportunities),
                    decay_guest_needs.after(initialize_spawned_guests),
                    guest_task_need_facts::observe_guest_need_triggers.after_ignore_deferred(decay_guest_needs),
                    advance_guest_survey_day,
                    guest_survey_need_sampling::sample_current_guest_need_observations.after_ignore_deferred(decay_guest_needs),
                    route_guest_survey_triggers,
                    adjust_guest_survey_data
                        .after_ignore_deferred(advance_guest_survey_day)
                        .after_ignore_deferred(guest_survey_need_sampling::sample_current_guest_need_observations)
                        .after_ignore_deferred(route_guest_survey_triggers),
                    react_to_visible_litter,
                    choose_guest_destination
                        .after(guest_destination_and_viewing_execution::project_authored_viewing_ground_slots)
                        .after_ignore_deferred(guest_task_need_facts::observe_guest_need_triggers)
                        .in_set(GuestDestinationSelection),
                    guest_self_task_selection::select_idle_guest_tasks
                        .after(choose_guest_destination)
                        .before(BehaviorStepFixedSet::Think),
                    guest_task_progress_diagnostics::report_guest_task_progress_changes
                        .after(guest_self_task_selection::select_idle_guest_tasks),
                )
                    .in_set(FixedGameSet::Think)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    spawn_arriving_guests.run_if(guest_arrival_execution::guest_arrivals_are_due),
                    guest_viewing_lifecycle::revoke_unowned_guest_observations,
                    begin_viewing
                        .after(guest_viewing_lifecycle::revoke_unowned_guest_observations)
                        .in_set(GuestViewingBegin),
                    advance_viewing.after(begin_viewing),
                    view_event_execution::apply_guest_reactions
                        .after(advance_viewing)
                        .in_set(GuestReactionApplication),
                    guest_view_event_meter_application::apply_accepted_view_events_to_view_meters
                        .after_ignore_deferred(view_event_execution::apply_guest_reactions),
                    cancel_failed_guest_destinations.after(begin_viewing),
                )
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (advance_guest_lifecycle.in_set(GuestLifecycleAdvance), age_guest_memories)
                    .chain()
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                despawn_departed_guests
                    .after(
                        openzt2_zoo_state::economy::EconomyServiceCleanupSet,
                    )
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(OnExit(GamePhase::InGame), remove_guest_survey_history);
    }
}

fn remove_guest_survey_history(mut commands: Commands) {
    commands.remove_resource::<GuestSurvey>();
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod guest_navigation_result_ownership_tests;

#[cfg(test)]
mod viewing_reaction_composition_tests;
