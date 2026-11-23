use bevy::prelude::*;
use bevy::{platform::collections::HashSet, utils::Parallel};

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::animal_health::types::Rampaging;
use openzt2_zoo_state::feeding::container_quantity::{DrinkContainer, FoodContainer};
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::locomotion::locomotion_types::{SpatialGrid, SpatialGridMembership};
use openzt2_zoo_state::maintenance::maintenance_types::FacilityContainedWaste;
use openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember, WorldMembers};
use openzt2_zoo_state::animal_behavior::behavior_target_retry_delays::BehaviorTargetRejections;

use super::{
    staff_assignment_types::{StaffAssignmentArea, StaffAssignmentTarget},
    staff_employment_types::{Staff, StaffJobCapabilities, StaffRole},
    staff_job_claim_ranking::RankedStaffJobClaimCandidate,
    staff_job_eligibility::{
        find_highest_priority_compatible_staff_behavior_task, staff_assignment_accepts_job_target,
        staff_subject_matches, StaffCandidateEligibilityFacts, StaffTasksByRequestToken,
    },
    staff_job_types::{
        CurrentJob, JobClaim, StaffJob, StaffJobBehaviorTask, StaffJobCancellationPending,
        StaffJobRouting, TankCleaningRequestSite, UnsuccessfulStaffJobCandidates,
    },
    worker_duty_assignment::{worker_duties_allow_staff_job, WorkerDutyFlags},
};
use openzt2_zoo_state::shows::running_show_types::AnimalPerformingShowTrick;

pub(crate) fn claim_highest_ranked_compatible_jobs_for_available_staff(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    target_retry_delays: BehaviorTargetRejections,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    loaded_behavior_documents: Res<LoadedBehaviorDocumentCollection>,
    token_tasks: Res<StaffTasksByRequestToken>,
    grids: Query<&SpatialGrid>,
    spatial_membership: SpatialGridMembership,
    terrain_surfaces: DrySurfaceEligibility,
    staff: Query<
        (
            Entity,
            &StaffRole,
            &StaffJobCapabilities,
            &WorldMember,
            &GlobalTransform,
            Option<&StaffAssignmentArea>,
            Option<&StaffAssignmentTarget>,
            WorkerDutyFlags,
        ),
        (With<Staff>, With<NavAgent>, Without<CurrentJob>),
    >,
    jobs: Query<
        (
            Entity,
            &StaffJob,
            &WorldMember,
            Option<&StaffJobRouting>,
            Option<&UnsuccessfulStaffJobCandidates>,
        ),
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
        Has<AnimalPerformingShowTrick>,
    )>,
    waste: Query<&FacilityContainedWaste>,
    filters: Query<&openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterFilter>,
    request_routing: (
        Query<(), With<TankCleaningRequestSite>>,
        Query<&WorldMembers>,
        Query<(Entity, &PersistentId, &GlobalTransform), With<Staff>>,
    ),
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
    let (request_sites, world_members, staff_recipients) = request_routing;
    let staff_manager = catalogue.staff_manager();
    let (job_safe_distance, steal_threshold) = staff_manager.map_or((6.0, 0.1), |policy| {
        (
            policy.job_safe_distance_m,
            policy.steal_job_threshold / 100.0,
        )
    });
    jobs.par_iter().for_each(|job_row| {
        let (job_entity, job, job_world, routing, unsuccessful) = job_row;
        let Some(token) = job.token else {
            return;
        };
        let routing = routing.copied().unwrap_or_default();
        let manager = staff_manager.filter(|_| routing.subject_type.is_some());
        let mut emitted_tokens = manager
            .into_iter()
            .flat_map(|policy| &policy.tokens)
            .filter(|rule| rule.token == token)
            .filter_map(|rule| rule.name);
        let first_token = if manager.is_some() {
            let Some(first_token) = emitted_tokens.next() else {
                return;
            };
            first_token
        } else {
            token
        };
        let other_tokens: Vec<AssetId> = emitted_tokens.collect();
        let Ok((
            target_transform,
            parent,
            habitat,
            target_rampaging,
            target_definition,
            food,
            drink,
            target_in_show,
        )) = targets.get(job.target)
        else {
            return;
        };
        let target_position = target_transform.translation();
        let recipient = routing.recipient.and_then(|identifier| {
            let members = world_members.get(job_world.root).ok()?;
            staff_recipients
                .iter_many(members.iter())
                .filter_map(Result::ok)
                .find_map(|(employee, id, transform)| {
                    (AssetId::from_key(&id.0.to_string()) == identifier).then(|| {
                        (
                            employee,
                            transform.translation().xz().distance(target_position.xz()),
                        )
                    })
                })
        });
        let Ok(spatial_grid) = grids.get(job_world.root) else {
            return;
        };
        let Some(target_cell) = spatial_grid.cell_of(target_position.xz()) else {
            return;
        };
        let Some(target_cell_coordinates) = spatial_grid.cell_xy(target_cell) else {
            return;
        };
        let target_parent = parent.map(ChildOf::parent);
        let target_area = habitat.map(|habitat| habitat.habitat_entity);
        let target_rampaging = target_rampaging.is_some();
        let target_on_land = terrain_surfaces.on_land_at_position(job_world.root, target_position);
        let candidate_facts = StaffCandidateEligibilityFacts {
            target_definition: target_definition.map(|definition| definition.0),
            target_food_level_q16: food
                .map(|container| container.amount_q16)
                .or_else(|| drink.map(|container| container.amount_q16)),
            target_in_water: target_on_land.map(|on_land| !on_land),
            target_on_land,
            target_rampaging: Some(target_rampaging),
            target_in_show: Some(target_in_show),
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
                    for candidate_entity in spatial_membership.members(job_world.root, cell) {
                        let Ok((
                            staff_entity,
                            staff_role,
                            capabilities,
                            staff_world,
                            staff_transform,
                            assigned_area,
                            assigned_target,
                            duties,
                        )) = staff.get(candidate_entity)
                        else {
                            continue;
                        };
                        if staff_world.root != job_world.root
                            || target_retry_delays.rejects(staff_entity, job.target)
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
                        if let Some((_, assigned_distance)) =
                            recipient.filter(|(recipient, _)| *recipient != staff_entity)
                        {
                            let candidate_distance = staff_transform
                                .translation()
                                .xz()
                                .distance(target_position.xz());
                            if assigned_distance <= job_safe_distance
                                || candidate_distance >= assigned_distance
                                || (assigned_distance - candidate_distance) / assigned_distance
                                    <= steal_threshold
                            {
                                continue;
                            }
                        }
                        let Some(role_definition) = catalogue.find_staff(staff_role.0) else {
                            continue;
                        };
                        if routing.subject_type.is_some_and(|required| {
                            !staff_subject_matches(catalogue, role_definition, required)
                        }) || job.kind.is_some_and(|kind| {
                            !worker_duties_allow_staff_job(
                                role_definition.role,
                                kind,
                                token,
                                *capabilities,
                                duties,
                            )
                        }) {
                            continue;
                        }
                        let resolve_task = |task_token| {
                            find_highest_priority_compatible_staff_behavior_task(
                                behavior_declarations,
                                &token_tasks,
                                catalogue,
                                role_definition,
                                task_token,
                                candidate_facts,
                            )
                        };
                        let Some(behavior) = resolve_task(first_token).and_then(|first| {
                            other_tokens.iter().try_fold(first, |best, &task_token| {
                                let task = resolve_task(task_token)?;
                                Some(
                                    if task
                                        .definition
                                        .priority
                                        .unwrap_or_default()
                                        .total_cmp(&best.definition.priority.unwrap_or_default())
                                        .is_gt()
                                    {
                                        task
                                    } else {
                                        best
                                    },
                                )
                            })
                        }) else {
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

    let mut candidates: Vec<_> = evaluated.drain().collect();
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
