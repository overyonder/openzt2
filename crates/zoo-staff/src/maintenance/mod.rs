use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorStepFixedSet;
use openzt2_zoo_state::maintenance::ZooCleanlinessProjection;
mod animal_waste_production;
mod authored_maintenance_facts_and_service_effects;
mod authored_trash_level_application;
mod first_person_staff_aim_presentation;
pub mod first_person_waste_cleaning;
mod loose_litter_sweeping_staff_job_execution;
mod maintenance_staff_job_claim_effect_progress_and_completion;
mod maintenance_staff_job_requests;
use openzt2_zoo_state::maintenance::maintenance_types;
mod staff_waste_termination;
mod waste_container_emptying_staff_job_execution;
pub(crate) mod zoo_cleanliness_projection;

use bevy::prelude::*;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    activated_behavior_actor, update_waiting_behavior_actors, RunBehaviorTermination,
};

use crate::staff::staff_job_behavior_start_and_completion::settle_staff_jobs_after_authored_behavior_finishes;
use crate::staff::staff_job_request_creation::create_or_raise_priority_of_requested_staff_jobs;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;

use self::{
    animal_waste_production::{
        bind_animal_waste_timers, produce_due_animal_habitat_waste, AnimalWasteTimer,
    },
    authored_maintenance_facts_and_service_effects::{
        apply_completed_service_maintenance_effects_to_facilities,
        project_authored_maintenance_capacity,
    },
    loose_litter_sweeping_staff_job_execution::advance_active_staff_litter_sweeping_jobs,
    maintenance_staff_job_requests::classify_spawned_staff_jobs_by_maintenance_work_kind,
    maintenance_types::ZooCleanlinessPermille,
    staff_waste_termination::{
        apply_staff_waste_termination, settle_consumed_staff_waste_jobs, WasteConsumptionClaim,
    },
    waste_container_emptying_staff_job_execution::advance_active_staff_waste_container_emptying_jobs,
    zoo_cleanliness_projection::{
        add_waste_contribution, project_zoo_cleanliness_from_totals_and_policy,
        publish_changed_waste_contributions, remove_waste_contribution,
        withdraw_disabled_waste_contribution, ZooCleanlinessTotals,
    },
};

pub struct MaintenancePlugin;

impl Plugin for MaintenancePlugin {
    fn build(&self, game_application: &mut App) {
        game_application
            .add_message::<openzt2_zoo_state::maintenance::maintenance_types::AdjustFacilityContainedWaste>()
            .register_required_components::<maintenance_types::LooseLitterWaste, WasteConsumptionClaim>()
            .register_required_components::<maintenance_types::AnimalHabitatWaste, WasteConsumptionClaim>()
            .register_required_components::<WorldRoot, ZooCleanlinessPermille>()
            .register_required_components::<WorldRoot, ZooCleanlinessTotals>();
        game_application
            .add_observer(add_waste_contribution)
            .add_observer(remove_waste_contribution)
            .add_observer(withdraw_disabled_waste_contribution);

        game_application
            .add_message::<first_person_waste_cleaning::AdvanceFirstPersonStaffActions>()
            .add_observer(first_person_waste_cleaning::play_first_person_staff_effect)
            .add_systems(
                Update,
                first_person_staff_aim_presentation::present_first_person_staff_aim
                    .after(first_person_waste_cleaning::refresh_first_person_staff_aim_and_use)
                    .in_set(GameSet::Presentation)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    first_person_waste_cleaning::publish_first_person_staff_action_delta
                        .after(openzt2_world_state::simulation_time::simulation_clock_types::ZooClockAdvance),
                    first_person_waste_cleaning::advance_first_person_staff_actions
                        .after_ignore_deferred(first_person_waste_cleaning::publish_first_person_staff_action_delta),
                )
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    first_person_waste_cleaning::hydrate_first_person_staff_actions,
                    first_person_waste_cleaning::refresh_first_person_staff_aim_and_use
                        .after(first_person_waste_cleaning::hydrate_first_person_staff_actions),
                )
                    .after(GameSet::Input)
                    .run_if(in_state(GamePhase::InGame)),
            );

        game_application
            .add_observer(authored_trash_level_application::apply_actor_trash_level_instructions)
            .add_observer(activated_behavior_actor::<RunBehaviorTermination>.pipe(apply_staff_waste_termination))
            .add_systems(
                FixedUpdate,
                authored_trash_level_application::apply_authored_trash_level_changes
                    .in_set(BehaviorStepFixedSet::Think)
                    .in_set(openzt2_zoo_state::maintenance::maintenance_types::FacilityWasteBehaviorModification),
            )
            .add_systems(
                FixedUpdate,
                update_waiting_behavior_actors.pipe(apply_staff_waste_termination)
                    .in_set(BehaviorStepFixedSet::Think),
            )
            .add_systems(
                FixedUpdate,
                settle_consumed_staff_waste_jobs
                    .after(BehaviorStepFixedSet::Act)
                    .in_set(FixedGameSet::Act)
                    .before(settle_staff_jobs_after_authored_behavior_finishes)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                project_authored_maintenance_capacity
                    .in_set(FixedGameSet::Think)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                classify_spawned_staff_jobs_by_maintenance_work_kind
                    .in_set(FixedGameSet::Think)
                    .after(create_or_raise_priority_of_requested_staff_jobs)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    bind_animal_waste_timers,
                    apply_completed_service_maintenance_effects_to_facilities.after(BehaviorStepFixedSet::Act),
                    produce_due_animal_habitat_waste
                        .after(bind_animal_waste_timers)
                        .run_if(any_with_component::<AnimalWasteTimer>),
                )
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    advance_active_staff_waste_container_emptying_jobs,
                    advance_active_staff_litter_sweeping_jobs,
                )
                    .in_set(FixedGameSet::Act)
                    .after(settle_staff_jobs_after_authored_behavior_finishes)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    publish_changed_waste_contributions,
                    project_zoo_cleanliness_from_totals_and_policy
                        .after(publish_changed_waste_contributions),
                )
                    .in_set(ZooCleanlinessProjection)
                    .in_set(FixedGameSet::Economy)
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}

#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "testing"))]
pub fn add_animal_waste_cleanup_test_systems(app: &mut App) {
    app.add_message::<openzt2_staff_requests::StaffControllerJobChange>()
        .register_required_components::<maintenance_types::AnimalHabitatWaste, WasteConsumptionClaim>()
        .add_observer(activated_behavior_actor::<RunBehaviorTermination>.pipe(apply_staff_waste_termination))
        .add_systems(
        Update,
        (
            create_or_raise_priority_of_requested_staff_jobs,
            classify_spawned_staff_jobs_by_maintenance_work_kind,
        )
            .chain()
            .in_set(FixedGameSet::Think),
    )
    .add_systems(
        Update,
        (
            settle_consumed_staff_waste_jobs,
        )
            .chain()
            .in_set(FixedGameSet::Act)
            .after(create_or_raise_priority_of_requested_staff_jobs)
            .before(settle_staff_jobs_after_authored_behavior_finishes),
    );
}
