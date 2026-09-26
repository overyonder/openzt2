//! Staff are ordinary world entities; work requests become contended job entities.

mod authored_initial_staff_animation_projection;
mod spawned_staff_identification;
mod staff_animal_health_job_execution;
mod staff_assignment_cleanup;
mod staff_assignment_interaction;
pub(crate) mod staff_assignment_types;
pub(crate) mod staff_behavior_fact_completion_execution;
mod staff_behavior_terminal_target_termination_acknowledgement;
pub(crate) mod staff_employment_types;
mod staff_facility_operation_job_execution;
mod staff_firing_lifecycle;
mod staff_hiring_lifecycle;
pub(crate) mod staff_job_behavior_start_and_completion;
mod staff_job_claim_ranking;
mod staff_job_claiming;
pub(crate) mod staff_job_completion_and_cancellation;
mod staff_job_eligibility;
pub(crate) mod staff_job_request_creation;
pub(crate) mod staff_job_types;
pub(crate) mod staff_lifecycle_messages;
mod staff_navigation_agent_projection;
mod staff_placement_interaction;
mod staff_request_candidate_cleanup;
mod staff_wage_lifecycle;
mod ui_actions;
mod worker_duty_assignment;

use bevy::prelude::*;

use staff_assignment_types::SetStaffWorkerDutyAssignment;
use staff_lifecycle_messages::{
    CancelStaffJobRequest, FireStaffRequest, HireStaffRequest, StaffFired, StaffHired,
    StaffJobCancelled, StaffJobCompleted, StaffJobRequest, TankWaterCleaned,
};

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::{FixedGameSet, GameSet};
use crate::plugins::construction::FixedConstructionSet;
use crate::plugins::placement::ObjectPlacementPreviewUpdateSet;

pub struct StaffPlugin;

impl Plugin for StaffPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<StaffJobRequest>()
            .add_message::<HireStaffRequest>()
            .add_message::<FireStaffRequest>()
            .add_message::<StaffHired>()
            .add_message::<StaffFired>()
            .add_message::<StaffJobCompleted>()
            .add_message::<StaffJobCancelled>()
            .add_message::<CancelStaffJobRequest>()
            .add_message::<TankWaterCleaned>()
            .add_message::<SetStaffWorkerDutyAssignment>()
            .add_systems(
                Update,
                (
                    authored_initial_staff_animation_projection::request_authored_initial_staff_animation_for_newly_resolved_prefab_models,
                    staff_placement_interaction::begin_staff_placement_from_staff_purchase_choice,
                    staff_placement_interaction::validate_staff_placement_preview_against_world_and_zoo_cash
                        .after(staff_placement_interaction::begin_staff_placement_from_staff_purchase_choice),
                    staff_placement_interaction::request_staff_hiring_from_confirmed_placement
                        .after(staff_placement_interaction::validate_staff_placement_preview_against_world_and_zoo_cash),
                    staff_placement_interaction::clear_staff_placement_after_leaving_placement_tool
                        .after(staff_placement_interaction::request_staff_hiring_from_confirmed_placement),
                    ui_actions::route_staff_assignment_selection_and_firing_ui_actions,
                    ui_actions::route_staff_assignment_ui_actions,
                    staff_assignment_interaction::assign_selected_staff_to_habitat_under_world_pointer
                        .after(ui_actions::route_staff_assignment_ui_actions),
                    staff_assignment_interaction::leave_staff_assignment_tools_after_cancel_action
                        .after(ui_actions::route_staff_assignment_ui_actions),
                    worker_duty_assignment::apply_staff_worker_duty_assignments
                        .after(ui_actions::route_staff_assignment_ui_actions),
                )
                    .in_set(GameSet::Intent)
                    .after(ObjectPlacementPreviewUpdateSet)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    spawned_staff_identification::identify_spawned_staff_from_world_definition,
                    worker_duty_assignment::enable_every_maintenance_worker_duty_for_new_staff_by_default,
                    staff_navigation_agent_projection::project_authored_staff_locomotion_onto_staff_entities,
                    staff_assignment_cleanup::clear_assignments_to_despawned_world_entities,
                    staff_job_request_creation::create_or_raise_priority_of_requested_staff_jobs,
                    staff_request_candidate_cleanup::clear_unsuccessful_candidates_at_authored_manager_interval,
                    staff_job_claiming::claim_highest_ranked_compatible_jobs_for_available_staff,
                    staff_job_behavior_start_and_completion::start_unstarted_authored_behavior_tasks_for_claimed_staff_jobs,
                )
                    .chain()
                    .in_set(FixedGameSet::Think),
            )
            .add_systems(
                FixedUpdate,
                (
                    staff_job_behavior_start_and_completion::release_staff_assignment_after_authored_behavior_failure_finishes,
                    staff_behavior_fact_completion_execution::acknowledge_staff_job_completion_fact_actions_owned_by_terminal_domain_systems,
                    staff_behavior_terminal_target_termination_acknowledgement::acknowledge_staff_target_termination_owned_by_terminal_maintenance_systems,
                    staff_job_behavior_start_and_completion::settle_staff_jobs_after_authored_behavior_finishes,
                    staff_facility_operation_job_execution::apply_completed_tank_maintenance_work,
                    staff_animal_health_job_execution::request_animal_health_effects_after_authored_staff_behavior_finishes,
                    staff_animal_health_job_execution::complete_staff_jobs_after_animal_health_effects_apply,
                )
                    .chain()
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                staff_wage_lifecycle::request_staff_wage_payments_on_first_day_of_month
                    .in_set(FixedGameSet::Economy),
            )
            .add_systems(
                FixedUpdate,
                staff_hiring_lifecycle::request_staff_hiring
                    .in_set(FixedConstructionSet::RequestEconomy),
            )
            .add_systems(
                FixedUpdate,
                (
                    staff_job_completion_and_cancellation::release_staff_and_despawn_completed_jobs,
                    staff_job_completion_and_cancellation::cancel_requested_jobs_and_jobs_with_invalid_world_relations,
                    staff_firing_lifecycle::release_claimed_job_and_despawn_fired_staff,
                    staff_wage_lifecycle::discard_settled_staff_wage_payment_operations,
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup),
            )
            .add_systems(
                FixedUpdate,
                (
                    staff_hiring_lifecycle::mark_paid_staff_hires_for_prefab_spawning,
                    staff_hiring_lifecycle::spawn_paid_staff_hires_from_ready_prefabs,
                    staff_hiring_lifecycle::discard_rejected_staff_hires,
                )
                    .chain()
                    .in_set(FixedConstructionSet::CompleteEconomy),
            );
    }
}

#[cfg(test)]
mod tests;
