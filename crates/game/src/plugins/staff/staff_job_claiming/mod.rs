use bevy::prelude::*;

use crate::assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use crate::assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_health::types::Rampaging;
use crate::plugins::habitat::habitat_types::HabitatMember;
use crate::plugins::locomotion::locomotion_types::NavAgent;
use crate::plugins::locomotion::locomotion_types::SpatialCell;
use crate::plugins::locomotion::locomotion_types::SpatialGrid;
use crate::plugins::terrain::dry_surface_eligibility::TerrainDrySurfaceEligibility;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

use super::{
    staff_assignment_types::{
        CleansFilters, CleansRecycling, EmptiesTrash, StaffAssignment, SweepsTrash,
    },
    staff_employment_types::{AvailableForWork, Staff, StaffRole},
    staff_job_claim_ranking::{
        insert_candidate_into_ranked_staff_job_claim_candidates, RankedStaffJobClaimCandidate,
        MAXIMUM_RANKED_STAFF_JOB_CANDIDATES,
    },
    staff_job_eligibility::{
        find_highest_priority_compatible_staff_behavior_task, staff_assignment_accepts_job_target,
        StaffCandidateEligibilityFacts,
    },
    staff_job_types::{
        CurrentJob, JobClaim, StaffJob, StaffJobBehaviorTask, UnsuccessfulStaffJobCandidates,
    },
    worker_duty_assignment::maintenance_worker_duties_accept_staff_job,
};

pub(in crate::plugins::staff) fn claim_highest_ranked_compatible_jobs_for_available_staff(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    loaded_behavior_documents: Res<LoadedBehaviorDocumentCollection>,
    spatial_grid: Res<SpatialGrid>,
    terrain_surfaces: TerrainDrySurfaceEligibility,
    staff: Query<
        (
            Entity,
            &StaffRole,
            &GlobalTransform,
            &SpatialCell,
            &StaffAssignment,
            Has<CleansFilters>,
            Has<CleansRecycling>,
            Has<EmptiesTrash>,
            Has<SweepsTrash>,
        ),
        (
            With<Staff>,
            With<AvailableForWork>,
            With<NavAgent>,
            Without<CurrentJob>,
        ),
    >,
    jobs: Query<(Entity, &StaffJob, Option<&UnsuccessfulStaffJobCandidates>), Without<JobClaim>>,
    targets: Query<(
        &GlobalTransform,
        Option<&ChildOf>,
        Option<&HabitatMember>,
        Option<&Rampaging>,
        &DefinitionId,
    )>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(behavior_declarations) =
        loaded_behavior_documents.create_declaration_index_view(&behavior_document_assets)
    else {
        return;
    };
    let mut available_staff = None;
    // Rebuild this scratch list every run so an unavailable pairing can be
    // reconsidered as soon as its eligibility facts change.
    let mut nearest_staff = Vec::new();
    let mut candidates = [None; MAXIMUM_RANKED_STAFF_JOB_CANDIDATES];
    for (job_entity, job, unsuccessful) in &jobs {
        // Token-less requests cannot select an authored behavior task.
        if job.token.is_none() {
            continue;
        }
        let Ok((target_transform, parent, habitat, target_rampaging, target_definition)) =
            targets.get(job.target)
        else {
            continue;
        };
        let target_position = target_transform.translation();
        let Some(target_cell) = spatial_grid.cell_of(target_position.xz()) else {
            continue;
        };
        let Some(target_cell_coordinates) = spatial_grid.cell_xy(target_cell) else {
            continue;
        };
        let available_staff = available_staff.get_or_insert_with(|| {
            let mut available_staff_entities = Vec::new();
            for (candidate_entity, _, _, staff_cell, _, _, _, _, _) in &staff {
                // The range check preserves membership in the prior grid snapshot.
                if spatial_grid.range(staff_cell.0).contains(&candidate_entity) {
                    available_staff_entities.push(candidate_entity);
                }
            }
            available_staff_entities
        });
        let target_parent = parent.map(ChildOf::parent);
        let target_area = habitat.map(|habitat| habitat.habitat_entity);
        let target_rampaging = target_rampaging.is_some();
        let target_on_land = terrain_surfaces.on_land(target_position.xz());
        let candidate_facts = StaffCandidateEligibilityFacts {
            target_in_water: target_on_land.map(|on_land| !on_land),
            target_on_land,
            target_rampaging: Some(target_rampaging),
            target_in_show: catalogue
                .find_object(target_definition.0)
                .and_then(|definition| (!definition.supports_show_tricks).then_some(false)),
        };

        nearest_staff.clear();
        let mut nearest_distance = None;
        // Keep the whole nearest compatible ring. The former row-major search
        // stopped after the first compatible row, biasing that row's candidates.
        for &candidate_entity in available_staff.iter() {
            if unsuccessful.is_some_and(|candidates| candidates.contains(candidate_entity)) {
                continue;
            }
            let Ok((
                staff_entity,
                staff_role,
                staff_transform,
                staff_cell,
                staff_assignment,
                cleans_filters,
                cleans_recycling,
                empties_trash,
                sweeps_trash,
            )) = staff.get(candidate_entity)
            else {
                continue;
            };
            // SpatialCell is navigation membership only; it is not a surface or show fact.
            let Some(staff_cell_coordinates) = spatial_grid.cell_xy(staff_cell.0) else {
                continue;
            };
            let distance = staff_cell_coordinates
                .x
                .abs_diff(target_cell_coordinates.x)
                .max(staff_cell_coordinates.y.abs_diff(target_cell_coordinates.y));
            if nearest_distance.is_some_and(|nearest| distance > nearest) {
                continue;
            }
            let Some(role_definition) = catalogue.find_staff(staff_role.0) else {
                continue;
            };
            if !maintenance_worker_duties_accept_staff_job(
                role_definition.role,
                job.kind,
                cleans_filters,
                cleans_recycling,
                empties_trash,
                sweeps_trash,
            ) {
                continue;
            }
            let Some(behavior_task) = find_highest_priority_compatible_staff_behavior_task(
                behavior_declarations,
                role_definition,
                job.kind,
                job.token,
                candidate_facts,
            ) else {
                continue;
            };
            if !staff_assignment_accepts_job_target(
                staff_assignment,
                job.target,
                target_parent,
                target_area,
            ) {
                continue;
            }

            match nearest_distance {
                Some(current) if distance == current => {}
                Some(_) => {
                    nearest_staff.clear();
                    nearest_distance = Some(distance);
                }
                None => nearest_distance = Some(distance),
            }
            nearest_staff.push((
                staff_entity,
                staff_transform
                    .translation()
                    .distance_squared(target_position),
                behavior_task.definition.priority.unwrap_or_default(),
            ));
        }

        for (staff_entity, distance_squared, authored_priority) in nearest_staff.drain(..) {
            insert_candidate_into_ranked_staff_job_claim_candidates(
                &mut candidates,
                RankedStaffJobClaimCandidate {
                    job: job_entity,
                    staff: staff_entity,
                    urgency: job.urgency,
                    authored_priority,
                    distance_squared,
                },
            );
        }
    }

    let mut claimed_jobs = [None; MAXIMUM_RANKED_STAFF_JOB_CANDIDATES];
    let mut claimed_staff = [None; MAXIMUM_RANKED_STAFF_JOB_CANDIDATES];
    let mut claim_count = 0;
    for candidate in candidates.iter().flatten().copied() {
        if claimed_jobs[..claim_count].contains(&Some(candidate.job))
            || claimed_staff[..claim_count].contains(&Some(candidate.staff))
        {
            continue;
        }
        let (Ok((_, job, _)), Ok((_, staff_role, _, _, _, _, _, _, _))) =
            (jobs.get(candidate.job), staff.get(candidate.staff))
        else {
            continue;
        };
        let Ok((target_transform, _, _, rampaging, definition)) = targets.get(job.target) else {
            continue;
        };
        let target_in_show = catalogue
            .find_object(definition.0)
            .and_then(|definition| (!definition.supports_show_tricks).then_some(false));
        let target_on_land = terrain_surfaces.on_land(target_transform.translation().xz());
        let Some(behavior_task) = catalogue.find_staff(staff_role.0).and_then(|role| {
            find_highest_priority_compatible_staff_behavior_task(
                behavior_declarations,
                role,
                job.kind,
                job.token,
                StaffCandidateEligibilityFacts {
                    target_in_water: target_on_land.map(|on_land| !on_land),
                    target_on_land,
                    target_rampaging: Some(rampaging.is_some()),
                    target_in_show,
                },
            )
        }) else {
            continue;
        };
        // Commit the reciprocal assignment only with its resolved authored task.
        commands.entity(candidate.job).insert((
            JobClaim {
                staff: candidate.staff,
            },
            StaffJobBehaviorTask {
                execution_id: None,
                document: behavior_task.document,
                task: behavior_task.definition.id,
                declaration_index: behavior_task.declaration_index,
            },
        ));
        commands
            .entity(candidate.staff)
            .insert(CurrentJob { job: candidate.job })
            .remove::<AvailableForWork>();
        claimed_jobs[claim_count] = Some(candidate.job);
        claimed_staff[claim_count] = Some(candidate.staff);
        claim_count += 1;
    }
}
