use bevy::prelude::*;
use bevy::{platform::collections::HashSet, utils::Parallel};

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_health::types::Rampaging;
use openzt2_zoo_state::feeding::container_quantity::{DrinkContainer, FoodContainer};
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::{SpatialGrid, SpatialGridMembership};
use openzt2_zoo_state::maintenance::maintenance_types::FacilityContainedWaste;
use openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_zoo_state::animal_behavior::behavior_target_retry_delays::BehaviorTargetRejections;

use super::{
    staff_assignment_types::{
        CleansFilters, CleansRecycling, EmptiesTrash, StaffAssignmentArea, StaffAssignmentTarget,
        SweepsTrash,
    },
    staff_employment_types::{Staff, StaffRole},
    staff_job_claim_ranking::RankedStaffJobClaimCandidate,
    staff_job_eligibility::{
        find_highest_priority_compatible_staff_behavior_task, staff_assignment_accepts_job_target,
        StaffCandidateEligibilityFacts, StaffTasksByRequestToken,
    },
    staff_job_types::{
        CurrentJob, JobClaim, StaffJob, StaffJobBehaviorTask, StaffJobCancellationPending,
        TankCleaningRequestSite, UnsuccessfulStaffJobCandidates,
    },
    worker_duty_assignment::maintenance_worker_duties_accept_staff_job,
};

pub(crate) fn claim_highest_ranked_compatible_jobs_for_available_staff(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    target_retry_delays: BehaviorTargetRejections,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    loaded_behavior_documents: Res<LoadedBehaviorDocumentCollection>,
    token_tasks: Res<StaffTasksByRequestToken>,
    spatial_grid: Res<SpatialGrid>,
    spatial_membership: SpatialGridMembership,
    terrain_surfaces: DrySurfaceEligibility,
    staff: Query<
        (
            Entity,
            &StaffRole,
            &GlobalTransform,
            Option<&StaffAssignmentArea>,
            Option<&StaffAssignmentTarget>,
            Has<CleansFilters>,
            Has<CleansRecycling>,
            Has<EmptiesTrash>,
            Has<SweepsTrash>,
        ),
        (With<Staff>, With<NavAgent>, Without<CurrentJob>),
    >,
    jobs: Query<
        (Entity, &StaffJob, Option<&UnsuccessfulStaffJobCandidates>),
        (Without<JobClaim>, Without<StaffJobCancellationPending>),
    >,
    targets: Query<(
        &GlobalTransform,
        Option<&ChildOf>,
        Option<&HabitatMember>,
        Option<&Rampaging>,
        Option<&DefinitionId>,
        Option<&FoodContainer>,
        Option<&DrinkContainer>,
    )>,
    waste: Query<&FacilityContainedWaste>,
    filters: Query<&openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterFilter>,
    request_sites: Query<(), With<TankCleaningRequestSite>>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(behavior_declarations) =
        loaded_behavior_documents.create_declaration_index_view(&behavior_document_assets)
    else {
        return;
    };
    if staff.is_empty() {
        return;
    }
    let mut evaluated =
        Parallel::<Vec<(RankedStaffJobClaimCandidate, StaffJobBehaviorTask)>>::default();
    jobs.par_iter().for_each(|(job_entity, job, unsuccessful)| {
        let Some(token) = job.token else {
            return;
        };
        let Ok((
            target_transform,
            parent,
            habitat,
            target_rampaging,
            target_definition,
            food,
            drink,
        )) = targets.get(job.target)
        else {
            return;
        };
        let target_position = target_transform.translation();
        let Some(target_cell) = spatial_grid.cell_of(target_position.xz()) else {
            return;
        };
        let Some(target_cell_coordinates) = spatial_grid.cell_xy(target_cell) else {
            return;
        };
        let target_parent = parent.map(ChildOf::parent);
        let target_area = habitat.map(|habitat| habitat.habitat_entity);
        let target_rampaging = target_rampaging.is_some();
        let target_on_land = terrain_surfaces.on_land_at_position(target_position);
        let candidate_facts = StaffCandidateEligibilityFacts {
            target_definition: target_definition.map(|definition| definition.0),
            target_food_level_q16: food
                .map(|container| container.amount_q16)
                .or_else(|| drink.map(|container| container.amount_q16)),
            target_in_water: target_on_land.map(|on_land| !on_land),
            target_on_land,
            target_rampaging: Some(target_rampaging),
            target_in_show: target_definition
                .and_then(|definition| catalogue.find_object(definition.0))
                .and_then(|definition| (!definition.supports_show_tricks).then_some(false)),
            target_trash_level_q16: waste
                .get(job.target)
                .ok()
                .map(|waste| i32::from(waste.contained_waste_units) << 16),
            target_filter_dirtiness_q16: filters
                .get(job.target)
                .ok()
                .map(|filter| (filter.dirt_percent * 65536.0) as i32),
            target_is_request_site: request_sites.contains(job.target),
        };

        evaluated.scope(|candidates| {
            let initial_count = candidates.len();
            let UVec2 {
                x: center_x,
                y: center_y,
            } = target_cell_coordinates;
            let edge = center_x
                .max(center_y)
                .max(spatial_grid.width - 1 - center_x)
                .max(spatial_grid.height - 1 - center_y);
            for ring in 0..=edge {
                let left = center_x.saturating_sub(ring);
                let right = center_x.saturating_add(ring).min(spatial_grid.width - 1);
                let top = center_y.saturating_sub(ring);
                let bottom = center_y.saturating_add(ring).min(spatial_grid.height - 1);
                let mut visit_cell = |x, y| {
                    let cell = y * spatial_grid.width + x;
                    for candidate_entity in spatial_membership.members(cell) {
                        let Ok((
                            staff_entity,
                            staff_role,
                            staff_transform,
                            assigned_area,
                            assigned_target,
                            cleans_filters,
                            cleans_recycling,
                            empties_trash,
                            sweeps_trash,
                        )) = staff.get(candidate_entity)
                        else {
                            continue;
                        };
                        if target_retry_delays.rejects(staff_entity, job.target)
                            || unsuccessful
                                .is_some_and(|candidates| candidates.contains(staff_entity))
                            || !staff_assignment_accepts_job_target(
                                assigned_area,
                                assigned_target,
                                job.target,
                                target_parent,
                                target_area,
                            )
                        {
                            continue;
                        }
                        let Some(role_definition) = catalogue.find_staff(staff_role.0) else {
                            continue;
                        };
                        if !maintenance_worker_duties_accept_staff_job(
                            role_definition.role,
                            job.kind,
                            Some(token),
                            cleans_filters,
                            cleans_recycling,
                            empties_trash,
                            sweeps_trash,
                        ) {
                            continue;
                        }
                        let Some(behavior) = find_highest_priority_compatible_staff_behavior_task(
                            behavior_declarations,
                            &token_tasks,
                            catalogue,
                            role_definition,
                            token,
                            candidate_facts,
                        ) else {
                            continue;
                        };
                        candidates.push((
                            RankedStaffJobClaimCandidate {
                                job: job_entity,
                                staff: staff_entity,
                                urgency: job.urgency,
                                authored_priority: behavior.definition.priority.unwrap_or_default(),
                                distance_squared: staff_transform
                                    .translation()
                                    .distance_squared(target_position),
                            },
                            StaffJobBehaviorTask {
                                execution_id: None,
                                document: behavior.document,
                                task: behavior.definition.id,
                                declaration_index: behavior.declaration_index,
                            },
                        ));
                    }
                };
                for y in top..=bottom {
                    if y.abs_diff(center_y) == ring {
                        for x in left..=right {
                            visit_cell(x, y);
                        }
                    } else {
                        if center_x >= ring {
                            visit_cell(left, y);
                        }
                        if right != left && right.abs_diff(center_x) == ring {
                            visit_cell(right, y);
                        }
                    }
                }
                if candidates.len() != initial_count {
                    break;
                }
            }
        });
    });

    let mut candidates: Vec<_> = evaluated.drain().flatten().collect();
    candidates.sort_unstable_by(|(left, _), (right, _)| left.claim_order(right));
    let mut claimed_jobs = HashSet::new();
    let mut claimed_staff = HashSet::new();
    for (candidate, behavior) in candidates {
        if claimed_jobs.contains(&candidate.job)
            || claimed_staff.contains(&candidate.staff)
            || !jobs.contains(candidate.job)
            || !staff.contains(candidate.staff)
        {
            continue;
        }
        commands.entity(candidate.job).insert((
            JobClaim {
                staff: candidate.staff,
            },
            behavior,
        ));
        claimed_jobs.insert(candidate.job);
        claimed_staff.insert(candidate.staff);
    }
}
