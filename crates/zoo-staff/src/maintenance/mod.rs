use openzt2_zoo_state::maintenance::ZooCleanlinessProjection;
mod animal_habitat_waste_cleaning_staff_job_execution;
mod animal_waste_production;
mod authored_maintenance_facts_and_service_effects;
mod authored_trash_level_application;
mod first_person_waste_cleaning;
mod loose_litter_sweeping_staff_job_execution;
mod maintenance_staff_job_claim_effect_progress_and_completion;
mod maintenance_staff_job_requests;
use openzt2_zoo_state::maintenance::maintenance_types;
mod waste_container_emptying_staff_job_execution;
pub(crate) mod zoo_cleanliness_projection;

use bevy::{ecs::schedule::common_conditions::on_message, prelude::*};
use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use crate::staff::staff_job_behavior_start_and_completion::settle_staff_jobs_after_authored_behavior_finishes;
use crate::staff::staff_job_request_creation::create_or_raise_priority_of_requested_staff_jobs;
use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;

use self::{
    animal_habitat_waste_cleaning_staff_job_execution::{
        apply_authored_staff_habitat_waste_termination,
        complete_applied_habitat_cleanup_after_behavior_exits,
    },
    animal_waste_production::{
        initialize_animal_waste_production_schedule, produce_due_animal_habitat_waste,
    },
    authored_maintenance_facts_and_service_effects::{
        apply_completed_service_maintenance_effects_to_facilities,
        initialize_authored_maintenance_facts_for_world_objects,
    },
    loose_litter_sweeping_staff_job_execution::advance_active_staff_litter_sweeping_jobs,
    maintenance_staff_job_requests::classify_spawned_staff_jobs_by_maintenance_work_kind,
    maintenance_types::ZooCleanlinessPermille,
    waste_container_emptying_staff_job_execution::advance_active_staff_waste_container_emptying_jobs,
    zoo_cleanliness_projection::{
        aggregate_changed_waste_into_zoo_cleanliness_totals,
        project_zoo_cleanliness_from_totals_and_policy, ZooCleanlinessTotals,
    },
};

pub struct MaintenancePlugin;

impl Plugin for MaintenancePlugin {
    fn build(&self, game_application: &mut App) {
        game_application
            .add_message::<openzt2_zoo_state::maintenance::maintenance_types::AdjustFacilityContainedWaste>()
            .init_resource::<ZooCleanlinessPermille>()
            .register_required_components::<WorldRoot, ZooCleanlinessTotals>();

        game_application.add_systems(
            Update,
            first_person_waste_cleaning::clean_nearest_waste_from_first_person_use_action
                .after(GameSet::Input)
                .run_if(on_message::<ActionRequest>)
                .run_if(in_state(GamePhase::InGame)),
        );

        game_application
            .add_systems(
                FixedUpdate,
                authored_trash_level_application::request_authored_trash_level_changes
                    .in_set(FixedGameSet::Think)
                    .in_set(openzt2_zoo_state::maintenance::maintenance_types::FacilityWasteBehaviorModification)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                initialize_authored_maintenance_facts_for_world_objects
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
                    initialize_animal_waste_production_schedule,
                    authored_trash_level_application::apply_authored_trash_level_changes,
                    apply_completed_service_maintenance_effects_to_facilities
                        .after_ignore_deferred(authored_trash_level_application::apply_authored_trash_level_changes),
                    produce_due_animal_habitat_waste
                        .after(initialize_animal_waste_production_schedule),
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
                    apply_authored_staff_habitat_waste_termination,
                    complete_applied_habitat_cleanup_after_behavior_exits,
                )
                    .chain()
                    .in_set(FixedGameSet::Act)
                    .before(settle_staff_jobs_after_authored_behavior_finishes)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    aggregate_changed_waste_into_zoo_cleanliness_totals,
                    project_zoo_cleanliness_from_totals_and_policy
                        .after_ignore_deferred(aggregate_changed_waste_into_zoo_cleanliness_totals),
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
    app.add_systems(
        Update,
        (
            create_or_raise_priority_of_requested_staff_jobs,
            classify_spawned_staff_jobs_by_maintenance_work_kind,
        )
            .chain()
            .in_set(FixedGameSet::Think),
    );
    app.add_systems(
        Update,
        (
            apply_authored_staff_habitat_waste_termination,
            complete_applied_habitat_cleanup_after_behavior_exits,
        )
            .chain()
            .in_set(FixedGameSet::Act)
            .after(create_or_raise_priority_of_requested_staff_jobs)
            .before(settle_staff_jobs_after_authored_behavior_finishes),
    );
}
