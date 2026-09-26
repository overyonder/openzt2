//! Visitor lifecycle represented directly by small ECS facts.

mod guest_animation_owner_projection;
mod guest_arrival_calculations;
pub(crate) mod guest_arrival_execution;
mod guest_behavior_fact_execution;
mod guest_definition_queries;
pub(crate) mod guest_destination_and_viewing_execution;
pub(crate) mod guest_lifecycle_and_memory_execution;
pub(crate) mod guest_need_execution;
mod guest_self_task_selection;
mod guest_simulation_calculations;
pub mod guest_simulation_types;
pub(crate) mod guest_survey_execution;
mod guest_target_eligibility;
mod guest_task_need_facts;
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
    advance_guest_lifecycle, age_guest_memories, apply_guest_reactions, despawn_departed_guests,
};
use guest_need_execution::{
    apply_behavior_guest_needs, apply_guest_satisfaction_adjustments, decay_guest_needs,
};
use guest_survey_execution::{
    adjust_guest_survey_data, initialize_guest_survey, survey_guest_needs,
};

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::{FixedGameSet, GameSet};
use guest_simulation_types::*;

pub struct GuestsPlugin;

impl Plugin for GuestsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<view_event_execution::BehaviorViewEventEmitted>()
            .add_message::<GuestReaction>()
            .add_message::<GuestReactionApplied>()
            .add_message::<AdjustGuestSatisfaction>()
            .add_message::<AdjustGuestAmusement>()
            .add_message::<AdjustGuestHunger>()
            .add_message::<AdjustGuestThirst>()
            .add_message::<AdjustGuestRest>()
            .add_message::<AdjustGuestRestroom>()
            .add_message::<AdjustGuestSurveyData>()
            .add_message::<SetAmusementPaintLevel>()
            .add_message::<GuestArrived>()
            .add_message::<GuestReachedEntrance>()
            .add_message::<GuestDeparted>()
            .add_message::<GuestArrivalFailed>()
            .add_message::<RequestGuestArrivals>()
            .add_systems(
                Update,
                (
                    initialize_guest_arrivals,
                    initialize_guest_survey,
                    report_changed_guest_arrival_failures,
                    adjust_guest_survey_data,
                    apply_guest_satisfaction_adjustments,
                    apply_behavior_guest_needs,
                )
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                guest_animation_owner_projection::project_guest_animation_owners
                    .in_set(GameSet::Presentation)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    initialize_spawned_guests,
                    hydrate_viewing_opportunities,
                    decay_guest_needs,
                    guest_behavior_fact_execution::apply_authored_guest_behavior_fact_modifications,
                    guest_task_need_facts::observe_guest_need_triggers,
                    survey_guest_needs,
                    react_to_visible_litter,
                    choose_guest_destination,
                    guest_self_task_selection::select_idle_guest_tasks,
                )
                    .chain()
                    .in_set(FixedGameSet::Think)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    view_event_execution::emit_authored_view_events,
                    spawn_arriving_guests,
                    begin_viewing,
                    advance_viewing,
                    apply_guest_reactions,
                    cancel_failed_guest_destinations,
                )
                    .chain()
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (advance_guest_lifecycle, age_guest_memories)
                    .chain()
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                despawn_departed_guests
                    .after(
                        crate::plugins::economy::service_cancellation::cancel_services_for_departed_guests_and_missing_facilities,
                    )
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(OnExit(GamePhase::InGame), remove_guest_arrival_state);
    }
}

fn remove_guest_arrival_state(mut commands: Commands) {
    commands.remove_resource::<GuestArrivalState>();
    commands.remove_resource::<GuestSurvey>();
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod guest_navigation_result_ownership_tests;
