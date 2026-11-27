//! Staff are ordinary world entities; work requests become contended job entities.

use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorStepFixedSet;
use openzt2_zoo_state::staff::StaffBehaviorSettlement;
use openzt2_zoo_state::staff::StaffJobRequestCreation;
mod authored_disease_cure_execution;

mod authored_initial_staff_animation_projection;
mod idle_staff_task_selection;
mod show_master_of_ceremonies;
mod spawned_staff_identification;
mod staff_animal_health_job_execution;
mod staff_assignment_cleanup;
pub mod staff_assignment_interaction;
pub mod staff_cognitive_state;
use openzt2_zoo_state::staff::staff_assignment_types;
pub(crate) mod staff_behavior_fact_completion_execution;
use openzt2_zoo_state::staff::staff_employment_types;
mod staff_facility_operation_job_execution;
mod staff_firing_lifecycle;
mod staff_hiring_lifecycle;
pub(crate) mod staff_job_behavior_start_and_completion;
mod staff_job_claim_ranking;
pub mod staff_job_claiming;
pub mod staff_job_completion_and_cancellation;
mod staff_job_eligibility;
pub(crate) mod staff_job_request_creation;
use openzt2_zoo_state::staff::staff_job_types;
use openzt2_zoo_state::staff::staff_lifecycle_messages;
mod staff_navigation_agent_projection;
mod staff_placement_interaction;
use openzt2_zoo_state::staff::staff_presentation_variant_selection;
pub mod staff_request_candidate_cleanup;
pub mod staff_request_token_delivery;
mod staff_wage_lifecycle;
pub(crate) mod staff_water_locomotion;
mod tank_cleaning_requests;
mod ui_actions;
mod worker_duty_assignment;

use bevy::prelude::*;

use staff_assignment_types::SetStaffWorkerDutyAssignment;
use staff_lifecycle_messages::{
    BeginStaffHirePlacement, CancelStaffJobRequest, FireStaffRequest, HireStaffRequest, StaffFired,
    StaffHired, StaffJobCancelled, StaffJobCompleted, StaffJobRequest,
};

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::construction::FixedConstructionSet;
use openzt2_zoo_state::placement::ObjectPlacementPreviewUpdateSet;

pub struct StaffPlugin;

impl Plugin for StaffPlugin {
    fn build(&self, app: &mut App) {
        show_master_of_ceremonies::register(app);
        app.register_required_components::<staff_employment_types::Staff, staff_assignment_types::DisabledTrainerTricks>();
        app.register_required_components::<openzt2_world_state::world_spawn::world_membership_types::WorldRoot, staff_request_token_delivery::StaffTokenTemplatePoints>();
        app.add_observer(staff_assignment_cleanup::release_staff_restrictions_when_target_leaves_world)
            .register_required_components::<staff_employment_types::Staff, staff_wage_lifecycle::LastRequestedWage>()
            .register_required_components::<staff_employment_types::Staff, staff_job_claiming::StaffRequestSearch>()
            .register_required_components_with::<staff_employment_types::Staff, staff_job_types::StaffCognitiveEligibility>(|| staff_job_types::StaffCognitiveEligibility {
                interruptible: true, on_tap: false, blocks_request_search: false,
            })
            .register_required_components::<staff_employment_types::Staff, staff_cognitive_state::FossilStaffSensor>()
            .register_required_components::<staff_employment_types::Staff, staff_cognitive_state::StaffNeedAdjustmentTimer>()
            .init_resource::<staff_job_eligibility::StaffTasksByRequestToken>()
            .add_message::<StaffJobRequest>()
            .add_message::<openzt2_staff_requests::StaffControllerJobChange>()
            .add_message::<openzt2_staff_requests::staff_request_fact_changes::StaffRequestFactChanged>()
            .add_message::<openzt2_world_state::simulation_time::simulation_clock_types::ZooTimekeeperElapsed>()
            .add_message::<openzt2_zoo_state::aquatic::aquatic_simulation_types::TankWaterCellsChanged>()
            .add_message::<openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterCellsChanged>()
            .add_message::<openzt2_zoo_state::habitat::habitat_cell_ownership_changes::HabitatCellOwnershipChanged>()
            .add_message::<BeginStaffHirePlacement>()
            .add_message::<HireStaffRequest>()
            .add_message::<FireStaffRequest>()
            .add_message::<StaffHired>()
            .add_message::<StaffFired>()
            .add_message::<StaffJobCompleted>()
            .add_message::<openzt2_zoo_state::animal_health::types::AnimalTreatmentSettled>()
            .add_message::<StaffJobCancelled>()
            .add_message::<CancelStaffJobRequest>()
            .add_message::<SetStaffWorkerDutyAssignment>()
            .add_message::<staff_cognitive_state::AdjustStaffNeed>()
            .add_message::<staff_cognitive_state::StaffNeedAdjustmentsDue>()
            .add_message::<staff_assignment_interaction::StaffAssignmentPlaced>()
            .add_message::<staff_assignment_interaction::RemoveSelectedStaffAssignment>()
            .add_observer(staff_request_token_delivery::clear_tokens_for_released_claim)
            .add_observer(authored_disease_cure_execution::apply_authored_disease_cures_for_live_staff_claims)
            .add_systems(
                Update,
                (
                    authored_initial_staff_animation_projection::request_authored_initial_staff_animation_for_newly_resolved_prefab_models,
                    authored_initial_staff_animation_projection::project_staff_animation_owners,
                    staff_placement_interaction::begin_staff_placement_from_hire_action,
                    staff_placement_interaction::validate_staff_placement_preview_against_world_and_zoo_cash
                        .after(staff_placement_interaction::begin_staff_placement_from_hire_action),
                    staff_placement_interaction::request_staff_hiring_from_confirmed_placement
                        .after(staff_placement_interaction::validate_staff_placement_preview_against_world_and_zoo_cash),
                    staff_placement_interaction::clear_staff_placement_after_leaving_placement_tool
                        .after(staff_placement_interaction::request_staff_hiring_from_confirmed_placement),
                    ui_actions::route_staff_assignment_selection_and_firing_ui_actions,
                    ui_actions::route_staff_assignment_ui_actions,
                    staff_assignment_interaction::capture_selected_staff_assignment
                        .after(ui_actions::route_staff_assignment_ui_actions),
                    staff_assignment_interaction::present_staff_assignment_placement
                        .after_ignore_deferred(staff_assignment_interaction::capture_selected_staff_assignment)
                        .run_if(on_message::<staff_assignment_interaction::StaffAssignmentPlaced>),
                    staff_assignment_interaction::retire_lost_trainer_assignment_rows,
                    staff_assignment_interaction::remove_selected_staff_assignment
                        .after(ui_actions::route_staff_assignment_selection_and_firing_ui_actions),
                    ui_actions::project_trainer_enabled_tricks
                        .after(staff_assignment_interaction::capture_selected_staff_assignment)
                        .after(staff_assignment_interaction::retire_lost_trainer_assignment_rows)
                        .after(staff_assignment_interaction::remove_selected_staff_assignment),
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
                staff_request_candidate_cleanup::project_staff_request_resource_coverage
                    .in_set(FixedGameSet::Clock),
            )
            .add_systems(
                FixedUpdate,
                (
                    spawned_staff_identification::identify_spawned_staff_from_world_definition,
                    staff_navigation_agent_projection::project_authored_staff_locomotion_onto_staff_entities
                        .after(spawned_staff_identification::identify_spawned_staff_from_world_definition),
                    tank_cleaning_requests::request_keeper_cleaning_for_dirty_tanks
                        .before(StaffJobRequestCreation),
                    staff_job_request_creation::create_or_raise_priority_of_requested_staff_jobs
                        .in_set(StaffJobRequestCreation),
                    staff_request_candidate_cleanup::clear_unsuccessful_candidates_at_authored_manager_interval
                        .after(StaffJobRequestCreation)
                        .after(openzt2_world_state::simulation_time::simulation_clock_types::ZooClockAdvance),
                    staff_job_eligibility::index_winning_staff_token_tasks,
                    staff_cognitive_state::project_staff_cognitive_eligibility
                        .before(staff_job_claiming::claim_highest_ranked_compatible_jobs_for_available_staff),
                    staff_request_token_delivery::project_staff_token_routing_settings,
                    staff_job_completion_and_cancellation::mark_explicit_staff_job_cancellations
                        .after(StaffJobRequestCreation)
                        .after_ignore_deferred(staff_request_candidate_cleanup::clear_unsuccessful_candidates_at_authored_manager_interval)
                        .run_if(on_message::<CancelStaffJobRequest>),
                    staff_job_claiming::claim_highest_ranked_compatible_jobs_for_available_staff
                        .after(staff_request_token_delivery::project_staff_token_routing_settings)
                        .after(openzt2_world_state::world_spawn::world_membership_types::target_type_index::WorldTargetTypePublication)
                        .after(staff_job_completion_and_cancellation::mark_explicit_staff_job_cancellations)
                        .after_ignore_deferred(staff_job_eligibility::index_winning_staff_token_tasks)
                        .after(staff_navigation_agent_projection::project_authored_staff_locomotion_onto_staff_entities)
                        .after_ignore_deferred(staff_request_candidate_cleanup::clear_unsuccessful_candidates_at_authored_manager_interval),
                    idle_staff_task_selection::interrupt_idle_staff_tasks_for_claimed_jobs
                        .after(staff_job_claiming::claim_highest_ranked_compatible_jobs_for_available_staff),
                    staff_job_behavior_start_and_completion::start_unstarted_authored_behavior_tasks_for_claimed_staff_jobs
                        .after(idle_staff_task_selection::interrupt_idle_staff_tasks_for_claimed_jobs),
                    idle_staff_task_selection::start_staff_token_tasks
                        .after(staff_job_behavior_start_and_completion::start_unstarted_authored_behavior_tasks_for_claimed_staff_jobs),
                    staff_cognitive_state::initialize_staff_needs
                        .after(spawned_staff_identification::identify_spawned_staff_from_world_definition),
                    staff_cognitive_state::publish_staff_need_adjustment_due
                        .after(staff_cognitive_state::initialize_staff_needs),
                    staff_cognitive_state::update_live_staff_needs
                        .in_set(openzt2_staff_requests::StaffRequestPropertyPublication)
                        .after_ignore_deferred(staff_cognitive_state::publish_staff_need_adjustment_due),
                    idle_staff_task_selection::start_authored_idle_staff_tasks
                        .after_ignore_deferred(staff_cognitive_state::update_live_staff_needs)
                        .after(idle_staff_task_selection::start_staff_token_tasks)
                        .before(BehaviorStepFixedSet::Think),
                )
                    .in_set(FixedGameSet::Think),
            )
            .add_systems(
                FixedUpdate,
                (
                    staff_behavior_fact_completion_execution::acknowledge_staff_job_completion_fact_actions_owned_by_terminal_domain_systems,
                ).in_set(BehaviorStepFixedSet::Think),
            )
            .add_systems(
                FixedUpdate,
                (
                    idle_staff_task_selection::collect_completed_staff_fossil_sites
                        .after(BehaviorStepFixedSet::Act),
                    staff_job_behavior_start_and_completion::release_staff_assignment_after_authored_behavior_failure_finishes
                        .after(BehaviorStepFixedSet::Act),
                    staff_job_behavior_start_and_completion::settle_staff_jobs_after_authored_behavior_finishes
                        .after(staff_job_behavior_start_and_completion::release_staff_assignment_after_authored_behavior_failure_finishes)
                        .in_set(StaffBehaviorSettlement),
                    staff_facility_operation_job_execution::apply_completed_tank_maintenance_work
                        .after(staff_job_behavior_start_and_completion::release_staff_assignment_after_authored_behavior_failure_finishes),
                    staff_animal_health_job_execution::request_animal_health_effects_after_authored_staff_behavior_finishes
                        .after(StaffBehaviorSettlement),
                    staff_animal_health_job_execution::complete_staff_jobs_after_animal_health_effects_apply
                        .after(staff_animal_health_job_execution::request_animal_health_effects_after_authored_staff_behavior_finishes),
                )
                    .in_set(FixedGameSet::Act),
            )
            .add_systems(
                FixedUpdate,
                staff_water_locomotion::project_staff_water_locomotion
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                staff_wage_lifecycle::request_monthly_wages
                    .run_if(on_message::<openzt2_world_state::simulation_time::simulation_clock_types::ZooDayAdvanced>)
                    .in_set(FixedGameSet::Economy),
            )
            .add_systems(
                FixedUpdate,
                staff_hiring_lifecycle::request_staff_hire_payment
                    .in_set(FixedConstructionSet::RequestEconomy),
            )
            .add_systems(
                FixedUpdate,
                (
                    staff_job_completion_and_cancellation::release_staff_and_despawn_completed_jobs,
                    staff_job_completion_and_cancellation::cancel_requested_jobs_and_jobs_with_invalid_world_relations
                        .after(staff_job_completion_and_cancellation::release_staff_and_despawn_completed_jobs),
                    staff_firing_lifecycle::release_claimed_job_and_despawn_fired_staff
                        .after(staff_job_completion_and_cancellation::cancel_requested_jobs_and_jobs_with_invalid_world_relations),
                    staff_wage_lifecycle::discard_settled_staff_wage_payment_operations,
                )
                    .in_set(FixedGameSet::Cleanup),
            )
            .add_systems(
                FixedUpdate,
                (
                    staff_hiring_lifecycle::accept_paid_employee_placements,
                    staff_hiring_lifecycle::spawn_paid_employees_from_ready_prefabs
                        .after(staff_hiring_lifecycle::accept_paid_employee_placements),
                    staff_hiring_lifecycle::discard_rejected_employee_placements,
                )
                    .in_set(FixedConstructionSet::CompleteEconomy),
            );
    }
}

#[cfg(test)]
mod tests;
