use openzt2_zoo_state::maintenance::ZooCleanlinessProjection;
mod animal_habitat_waste_cleaning_staff_job_execution;
mod animal_waste_production;
mod cleanliness_aggregation_operations;
mod loose_litter_sweeping_staff_job_execution;
mod maintainable_object_condition_and_service_effects;
mod maintenance_repair_staff_job_execution;
mod maintenance_staff_job_claim_effect_progress_and_completion;
mod maintenance_staff_job_requests;
use openzt2_zoo_state::maintenance::maintenance_types;
mod waste_container_emptying_staff_job_execution;
pub(crate) mod zoo_cleanliness_projection;

use bevy::prelude::*;

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
    cleanliness_aggregation_operations::{
        add_animal_habitat_waste_to_cleanliness_totals,
        add_facility_waste_container_to_cleanliness_totals, add_loose_litter_to_cleanliness_totals,
        add_maintainable_object_condition_to_cleanliness_totals,
        remove_animal_habitat_waste_from_cleanliness_totals,
        remove_facility_waste_container_from_cleanliness_totals,
        remove_loose_litter_from_cleanliness_totals,
        remove_maintainable_object_condition_from_cleanliness_totals,
    },
    loose_litter_sweeping_staff_job_execution::advance_active_staff_litter_sweeping_jobs,
    maintainable_object_condition_and_service_effects::{
        apply_completed_service_maintenance_effects_to_facilities,
        deteriorate_maintainable_world_objects_after_zoo_days_advance,
        initialize_authored_maintenance_facts_for_world_objects,
    },
    maintenance_repair_staff_job_execution::advance_active_staff_repair_jobs,
    maintenance_staff_job_requests::{
        classify_spawned_staff_jobs_by_maintenance_work_kind,
        request_staff_cleaning_for_spawned_habitat_waste,
        request_staff_emptying_for_full_waste_containers,
        request_staff_repairs_for_damaged_maintainable_objects,
        request_staff_sweeping_for_spawned_litter,
        requeue_cancelled_maintenance_staff_work_when_still_required,
    },
    maintenance_types::{
        AnimalHabitatWaste, FacilityContainedWaste, IncrementalZooCleanlinessTotals,
        LooseLitterWaste, MaintainableObjectConditionPermille, ZooCleanlinessPermille,
    },
    waste_container_emptying_staff_job_execution::advance_active_staff_waste_container_emptying_jobs,
    zoo_cleanliness_projection::project_changed_maintenance_totals_into_zoo_cleanliness,
};

pub struct MaintenancePlugin;

impl Plugin for MaintenancePlugin {
    fn build(&self, game_application: &mut App) {
        game_application
            .init_resource::<IncrementalZooCleanlinessTotals>()
            .init_resource::<ZooCleanlinessPermille>();

        game_application
            .world_mut()
            .register_component_hooks::<MaintainableObjectConditionPermille>()
            .on_add(add_maintainable_object_condition_to_cleanliness_totals)
            .on_remove(remove_maintainable_object_condition_from_cleanliness_totals);
        game_application
            .world_mut()
            .register_component_hooks::<FacilityContainedWaste>()
            .on_add(add_facility_waste_container_to_cleanliness_totals)
            .on_remove(remove_facility_waste_container_from_cleanliness_totals);
        game_application
            .world_mut()
            .register_component_hooks::<LooseLitterWaste>()
            .on_add(add_loose_litter_to_cleanliness_totals)
            .on_remove(remove_loose_litter_from_cleanliness_totals);
        game_application
            .world_mut()
            .register_component_hooks::<AnimalHabitatWaste>()
            .on_add(add_animal_habitat_waste_to_cleanliness_totals)
            .on_remove(remove_animal_habitat_waste_from_cleanliness_totals);

        game_application
            .add_systems(
                FixedUpdate,
                (
                    initialize_authored_maintenance_facts_for_world_objects,
                    deteriorate_maintainable_world_objects_after_zoo_days_advance,
                )
                    .chain()
                    .in_set(FixedGameSet::Think)
                    .before(request_staff_repairs_for_damaged_maintainable_objects)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    request_staff_repairs_for_damaged_maintainable_objects,
                    request_staff_emptying_for_full_waste_containers,
                    request_staff_sweeping_for_spawned_litter,
                    request_staff_cleaning_for_spawned_habitat_waste,
                    requeue_cancelled_maintenance_staff_work_when_still_required,
                )
                    .in_set(FixedGameSet::Think)
                    .before(create_or_raise_priority_of_requested_staff_jobs)
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
                    apply_completed_service_maintenance_effects_to_facilities,
                    produce_due_animal_habitat_waste,
                )
                    .chain()
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    advance_active_staff_repair_jobs,
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
                project_changed_maintenance_totals_into_zoo_cleanliness
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
    app.init_resource::<IncrementalZooCleanlinessTotals>();
    app.world_mut()
        .register_component_hooks::<AnimalHabitatWaste>()
        .on_add(add_animal_habitat_waste_to_cleanliness_totals)
        .on_remove(remove_animal_habitat_waste_from_cleanliness_totals);
    app.add_systems(
        Update,
        (
            request_staff_cleaning_for_spawned_habitat_waste,
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
