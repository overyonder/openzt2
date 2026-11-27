use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::animal_behavior::behavior_token_types::{BehaviorSelfToken, BehaviorTokenContext, BehaviorTokenXmlPayload};
use openzt2_zoo_state::animal_health::types::{Disease, Rampaging};
use openzt2_zoo_state::feeding::container_quantity::{DrinkContainer, FoodContainer};
use openzt2_zoo_state::habitat::{
    habitat_membership_and_containment::locate_habitat_at_world_position,
    habitat_types::{HabitatCellOf, HabitatIndex, HabitatMember},
};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use super::staff_request_token_delivery::{
    StaffTokenDispatch, StaffTokenRoutingSettings,
    StaffTokenTemplatePoints,
};
use super::staff_assignment_interaction::assignment_constraint_matches;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::maintenance::maintenance_types::FacilityContainedWaste;
use openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::{
    target_type_index::WorldTargetTypeQueries, DefinitionId, WorldMember,
};
use openzt2_zoo_state::animal_behavior::behavior_target_retry_delays::BehaviorTargetRejections;

use super::{
    staff_assignment_types::{
        StaffAssignmentArea, StaffAssignmentRecord, StaffAssignmentTarget, WorkerDutyAssignments,
    },
    staff_employment_types::{Staff, StaffRole},
    staff_job_claim_ranking::{
        can_steal_request, request_age, staff_search_retry_deadline, StaffRequestRank,
    },
    staff_job_eligibility::{
        find_highest_priority_compatible_staff_behavior_task, staff_assignment_accepts_job_target,
        staff_subject_matches, StaffCandidateEligibilityFacts, StaffTasksByRequestToken,
    },
    staff_job_types::{
        CurrentJob, JobClaim, StaffCognitiveEligibility, StaffJob, StaffJobBehaviorTask,
        StaffJobCancellationPending, StaffJobRouting, StaffJobTarget, StaffLocalViewingArea,
        TankCleaningRequestSite, UnsuccessfulStaffJobCandidates,
    },
};
use openzt2_zoo_state::shows::running_show_types::AnimalPerformingShowTrick;

use openzt2_world_state::simulation_time::{
    native_visual_random_stream::NativeVisualRandomStream, simulation_clock_types::ZooClock,
};
use openzt2_zoo_state::behavior_task_execution_types::{
    abandon_behavior_task_for_reconsideration, BehaviorTaskExecutionState,
    SynchronizedBehaviorParticipant,
};

#[derive(Component, Default)]
pub struct StaffRequestSearch {
    retry_at: f64,
    last_ordinary_assignment: f64,
}

impl StaffRequestSearch {
    pub fn saved_state(&self) -> [f64; 2] {
        [self.retry_at, self.last_ordinary_assignment]
    }

    pub fn from_saved_state([retry_at, last_ordinary_assignment]: [f64; 2]) -> Self {
        Self {
            retry_at,
            last_ordinary_assignment,
        }
    }
}

pub(super) fn claim_highest_ranked_compatible_jobs_for_available_staff(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    target_retry_delays: BehaviorTargetRejections,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    loaded_behavior_documents: Res<LoadedBehaviorDocumentCollection>,
    token_tasks: Res<StaffTasksByRequestToken>,
    mut random: ResMut<NativeVisualRandomStream>,
    (clocks, staff_tokens, local_viewing_areas): (
        Query<(
            &ZooClock,
            Option<
                &openzt2_world_state::simulation_time::simulation_control_types::SimulationPaused,
            >,
        )>,
        Query<(
            &BehaviorSelfToken,
            Option<&BehaviorTokenContext>,
            Has<BehaviorTokenXmlPayload>,
        )>,
        Query<(), With<StaffLocalViewingArea>>,
    ),
    (terrain_surfaces, staff_tasks): (
        DrySurfaceEligibility,
        Query<&BehaviorTaskExecutionState, With<Staff>>,
    ),
    mut staff: Query<
        (
            Entity,
            &StaffRole,
            &WorldMember,
            &GlobalTransform,
            Option<&StaffAssignmentArea>,
            Option<&StaffAssignmentTarget>,
            &WorkerDutyAssignments,
            &PersistentId,
            &mut StaffRequestSearch,
            Option<&Children>,
            Option<&CurrentJob>,
            Option<&StaffCognitiveEligibility>,
        ),
        (
            With<Staff>,
            With<NavAgent>,
            Without<SynchronizedBehaviorParticipant>,
        ),
    >,
    mut jobs: ParamSet<(
        Query<
            (
                Entity,
                &StaffJob,
                &WorldMember,
                Option<&StaffJobRouting>,
                Option<&UnsuccessfulStaffJobCandidates>,
                Option<&JobClaim>,
                &PersistentId,
                Option<&StaffJobTarget>,
            ),
            Without<StaffJobCancellationPending>,
        >,
        Query<Option<&mut UnsuccessfulStaffJobCandidates>, With<StaffJob>>,
    )>,
    targets: Query<(
        &GlobalTransform,
        Option<&ChildOf>,
        Option<&HabitatMember>,
        Option<&Rampaging>,
        Option<&DefinitionId>,
        Option<&FoodContainer>,
        Option<&DrinkContainer>,
        Has<AnimalPerformingShowTrick>,
        &WorldMember,
        Option<&Disease>,
    )>,
    assignments: (
        Query<(&StaffAssignmentRecord, Option<&StaffAssignmentTarget>)>,
        Query<(&HabitatIndex, &TopologyGrid)>,
        Query<&HabitatCellOf>,
    ),
    (waste, filters): (
        Query<&FacilityContainedWaste>,
        Query<&openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterFilter>,
    ),
    request_routing: (
        Query<(), With<TankCleaningRequestSite>>,
        Query<(
            Option<&StaffTokenRoutingSettings>,
            &mut StaffTokenTemplatePoints,
        )>,
        WorldTargetTypeQueries,
        Query<(Entity, &Name, &WorldMember)>,
        Query<(&GlobalTransform, &WorldMember, Option<&PersistentId>)>,
        Query<
            (
                Entity,
                &PersistentId,
                &GlobalTransform,
                &WorldMember,
                Has<SynchronizedBehaviorParticipant>,
                Option<&StaffCognitiveEligibility>,
            ),
            With<Staff>,
        >,
    ),
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    let (Some(behavior_declarations), Some(manager), Some(timestep)) = (
        loaded_behavior_documents.create_declaration_index_view(&behavior_document_assets),
        catalogue.staff_manager(),
        catalogue.simulation_timestep(),
    ) else {
        return;
    };

    let (
        request_sites,
        mut token_settings,
        recipient_types,
        recipient_names,
        recipients,
        staff_positions,
    ) = request_routing;
    let mut addressed = Vec::new();
    let mut workers: Vec<_> = staff
        .iter()
        .map(|(worker, _, _, _, _, _, _, id, ..)| (id.0, worker))
        .collect();
    workers.sort_unstable();
    let jobs_view = jobs.p0();
    let mut requests: Vec<_> = jobs_view.iter().collect();
    let mut mismatches = Vec::new();
    requests.sort_unstable_by_key(|(_, _, _, _, _, _, id, ..)| id.0);
    let mut assigned_this_scan = HashMap::<Entity, Entity>::new();
    let mut retired = HashSet::new();
    let cure = AssetId::from_key("t_cureanimal");

    'workers: for (_, worker) in workers {
        let Ok((
            _,
            role,
            member,
            transform,
            assigned_area,
            assigned_target,
            duties,
            _,
            mut sensor,
            children,
            current,
            cognitive,
        )) = staff.get_mut(worker)
        else {
            continue;
        };
        let Ok((clock, pause)) = clocks.get(member.root) else {
            continue;
        };
        if pause.is_some_and(|pause| pause.0) {
            continue;
        }
        let mut has_live_request_token = false;
        let mut has_invalid_request_token = false;
        let recognized = |name| manager.tokens.iter().any(|rule| rule.name == Some(name));
        for token in children.into_iter().flat_map(|children| children.iter()) {
            let Ok((token, context, has_xml)) = staff_tokens.get(token) else {
                continue;
            };
            if recognized(token.name) && token.expires_at > clock.tick {
                if (token.payload.is_none() && has_xml)
                    || token.payload.is_some_and(|target| targets.contains(target))
                    || context
                        .and_then(|context| context.point)
                        .is_some_and(|point| point != Vec3::splat(-1.0))
                {
                    has_live_request_token = true;
                } else {
                    has_invalid_request_token = true;
                }
            }
        }
        if has_invalid_request_token {
            if let Some(job) = current.map(CurrentJob::job) {
                super::staff_request_token_delivery::remove_staff_request_tokens(
                    job,
                    &mut commands,
                );
                commands.entity(job).remove::<JobClaim>();
                abandon_behavior_task_for_reconsideration(worker, &mut commands);
            }
        } else if has_live_request_token
            || current.is_some_and(|current| {
                jobs_view
                    .get(current.job())
                    .is_ok_and(|(_, _, _, routing, ..)| {
                        routing.is_none_or(|route| route.subject_type.is_none())
                    })
            })
        {
            continue;
        }
        let now = clock.tick as f64 * timestep.as_secs_f64();
        if now <= sensor.retry_at || cognitive.is_some_and(|state| state.blocks_request_search) {
            continue;
        }
        let Some(role) = catalogue.find_staff(role.0) else {
            continue;
        };
        let Some(selection) = catalogue.behavior_selection_policy() else {
            continue;
        };
        if !selection.enabled {
            continue;
        }
        if cognitive.is_some_and(|state| !state.interruptible || state.on_tap) {
            sensor.retry_at = staff_search_retry_deadline(now, random.next_u15());
            continue;
        }
        let origin = transform.translation().xz();
        let mut best = StaffRequestRank::INITIAL;
        let mut prefers_local_area = false;
        let mut winner = None;
        for &(job_entity, job, job_world, routing, unsuccessful, claim, _, live_target) in &requests
        {
            if job_world.root != member.root || retired.contains(&job_entity) {
                continue;
            }
            let routing = routing.copied().unwrap_or_default();
            let token = job.token;
            let manager_rows = || {
                manager
                    .tokens
                    .iter()
                    .filter(|rule| Some(rule.token) == token)
            };
            let authored = routing.subject_type.is_some();
            let Some(token) = token.filter(|_| !authored || manager_rows().next().is_some()) else {
                if let Some(claim) = claim {
                    abandon_behavior_task_for_reconsideration(claim.staff, &mut commands);
                }
                commands.entity(job_entity).try_despawn();
                retired.insert(job_entity);
                continue;
            };
            let target = live_target.map(|target| target.0);
            let target_info = target.and_then(|target| targets.get(target).ok());
            if target.is_some()
                && target_info
                    .as_ref()
                    .is_none_or(|facts| facts.8.root != member.root)
            {
                if let Some(claim) = claim {
                    abandon_behavior_task_for_reconsideration(claim.staff, &mut commands);
                    commands.entity(job_entity).remove::<JobClaim>();
                }
                continue;
            }
            let Some(point) =
                job.destination(target_info.as_ref().map(|facts| facts.0.translation()))
            else {
                if let Some(claim) = claim {
                    abandon_behavior_task_for_reconsideration(claim.staff, &mut commands);
                }
                commands.entity(job_entity).try_despawn();
                retired.insert(job_entity);
                continue;
            };
            let (parent, habitat, rampaging, definition, food, drink, in_show, disease) =
                target_info.map_or(
                    (None, None, None, None, None, None, false, None),
                    |(
                        _,
                        parent,
                        habitat,
                        rampaging,
                        definition,
                        food,
                        drink,
                        in_show,
                        _,
                        disease,
                    )| {
                        (
                            parent, habitat, rampaging, definition, food, drink, in_show, disease,
                        )
                    },
                );
            if !assigned_this_scan.contains_key(&job_entity)
                && claim.is_some_and(|claim| claim.staff == worker)
            {
                let same_destination = staff_tasks.get(worker).is_ok_and(|task| {
                    let token = task.object.and_then(|token| staff_tokens.get(token).ok());
                    let task_target = token.map_or(task.target, |(token, ..)| token.payload);
                    let task_point = token.and_then(|(_, context, _)| context?.point);
                    task_target == target && task_point == job.point
                });
                if same_destination {
                    sensor.retry_at = staff_search_retry_deadline(now, random.next_u15());
                    continue 'workers;
                }
                super::staff_request_token_delivery::remove_staff_request_tokens(
                    job_entity,
                    &mut commands,
                );
                commands.entity(job_entity).remove::<JobClaim>();
                abandon_behavior_task_for_reconsideration(worker, &mut commands);
                mismatches.push((job_entity, worker));
                continue;
            }
            if job.remaining_wait_seconds > 0.0
                || (token == cure
                    && disease.is_none_or(|disease| disease.definition == AssetId::from_key("")))
                || (job.point.is_none()
                    && target.is_some_and(|target| target_retry_delays.rejects(worker, target)))
                || unsuccessful.is_some_and(|attempts| attempts.contains(worker))
                || routing
                    .subject_type
                    .is_some_and(|required| !staff_subject_matches(catalogue, role, required))
                || !duties.is_assigned(token)
            {
                continue;
            }
            let (assignment_rows, topology, cell_owners) = &assignments;
            let area_at = |point: Vec3| {
                let (index, grid) = topology.get(member.root).ok()?;
                locate_habitat_at_world_position(index, cell_owners, point.xz(), *grid)
            };
            let target_area = habitat
                .map(|habitat| habitat.habitat_entity)
                .or_else(|| area_at(point));
            let mut rows = children
                .into_iter()
                .flat_map(|children| children.iter())
                .filter_map(|row| assignment_rows.get(row).ok())
                .peekable();
            let target_entity = target;
            let assigned = if rows.peek().is_some() {
                rows.any(|(record, target)| match record {
                    StaffAssignmentRecord::Point(assigned) => assignment_constraint_matches(
                        0,
                        [
                            Some(
                                area_at(*assigned)
                                    .zip(target_area)
                                    .is_some_and(|(a, b)| a == b),
                            ),
                            None,
                            None,
                        ],
                    ),
                    StaffAssignmentRecord::Animal => assignment_constraint_matches(
                        0,
                        [
                            None,
                            Some(target.is_some_and(|assigned| Some(assigned.0) == target_entity)),
                            None,
                        ],
                    ),
                })
            } else {
                target.map_or_else(
                    || {
                        assigned_target.is_none()
                            && assigned_area.is_none_or(|area| Some(area.0) == target_area)
                    },
                    |target| {
                        staff_assignment_accepts_job_target(
                            assigned_area,
                            assigned_target,
                            target,
                            parent.map(ChildOf::parent),
                            target_area,
                        )
                    },
                )
            };
            if !assigned {
                continue;
            }
            let distance = origin.distance(point.xz());
            let incumbent = assigned_this_scan
                .get(&job_entity)
                .copied()
                .or_else(|| claim.map(|claim| claim.staff));
            let preferred_recipient = || {
                let key = routing.recipient?;
                staff_positions
                    .iter()
                    .find_map(|(entity, id, _, world, _, _)| {
                        (world.root == member.root && AssetId::from_key(&id.0.to_string()) == key)
                            .then_some(entity)
                    })
            };
            if let Some(other) = incumbent
                .or_else(preferred_recipient)
                .filter(|&other| other != worker)
            {
                if staff_positions
                    .get(other)
                    .is_ok_and(|(_, _, pose, _, blocked, cognitive)| {
                        blocked
                            || cognitive.is_some_and(|state| !state.interruptible || state.on_tap)
                            || !can_steal_request(
                                distance,
                                pose.translation().xz().distance(point.xz()),
                                manager.job_safe_distance_m,
                                manager.steal_job_threshold,
                            )
                    })
                {
                    continue;
                }
            }
            let issued_at = f64::from(job.issued_at_seconds);
            let rank = StaffRequestRank {
                priority: job.priority,
                age: request_age(now, issued_at, manager.time_category_duration),
                distance,
            };
            let local = best.priority > 1.0
                && rank.priority > 1.0
                && sensor.last_ordinary_assignment > 0.0
                && sensor.last_ordinary_assignment > issued_at
                && staff_subject_matches(catalogue, role, AssetId::from_key("keeper"))
                && area_at(transform.translation())
                    .zip(target_area)
                    .is_some_and(|(ours, target)| {
                        ours == target && local_viewing_areas.contains(ours)
                    });
            if prefers_local_area && rank.priority > 1.0 && !local {
                continue;
            }
            if !(rank.outranks(best) || (local && !prefers_local_area)) {
                continue;
            }
            let on_land = terrain_surfaces.on_land_at_position(member.root, point);
            let facts = StaffCandidateEligibilityFacts {
                target_definition: definition.map(|definition| definition.0),
                target_food_level_q16: food
                    .map(|value| value.amount_q16)
                    .or_else(|| drink.map(|value| value.amount_q16)),
                target_in_water: on_land.map(|on_land| !on_land),
                target_on_land: on_land,
                target_rampaging: Some(rampaging.is_some()),
                target_in_show: Some(in_show),
                target_trash_level_q16: target
                    .and_then(|target| waste.get(target).ok())
                    .map(|value| i32::from(value.contained_waste_units) << 16),
                target_filter_dirtiness_q16: target
                    .and_then(|target| filters.get(target).ok())
                    .map(|value| (value.dirt_percent * 65_536.0) as i32),
                target_is_request_site: target.is_some_and(|target| request_sites.contains(target)),
            };
            let resolve_task = |name| {
                find_highest_priority_compatible_staff_behavior_task(
                    behavior_declarations,
                    &token_tasks,
                    catalogue,
                    role,
                    name,
                    facts,
                )
            };
            let mut task = None;
            if authored {
                let mut all_satisfied = true;
                for rule in manager_rows() {
                    let compatible = rule.name.and_then(resolve_task);
                    if target.is_some() && compatible.is_none() {
                        all_satisfied = false;
                        break;
                    }
                }
                if !all_satisfied {
                    continue;
                }
            } else {
                task = resolve_task(token);
                if task.is_none() {
                    continue;
                }
            }
            best = rank;
            prefers_local_area = local;
            winner = Some((
                job_entity, target, token, incumbent, task, authored, job.point, facts,
            ));
        }
        let Some((job, target, token, incumbent, mut task, authored, point, facts)) = winner else {
            sensor.retry_at = staff_search_retry_deadline(now, random.next_u15());
            continue;
        };
        if incumbent == Some(worker) {
            sensor.retry_at = staff_search_retry_deadline(now, random.next_u15());
            continue;
        }
        if let Some(other) = incumbent {
            super::staff_request_token_delivery::remove_staff_request_tokens(job, &mut commands);
            abandon_behavior_task_for_reconsideration(other, &mut commands);
            commands.entity(job).remove::<(
                JobClaim,
                StaffJobBehaviorTask,
                super::staff_request_token_delivery::StaffRequestExecutionToken,
            )>();
        }
        if !prefers_local_area {
            sensor.last_ordinary_assignment = now;
        }
        commands.entity(job).insert(JobClaim { staff: worker });
        assigned_this_scan.insert(job, worker);
        let delivered = if authored {
            token_settings
                .get_mut(member.root)
                .ok()
                .and_then(|(settings, mut template_points)| {
                    super::staff_request_token_delivery::deliver_staff_request_tokens(
                        StaffTokenDispatch {
                            job,
                            worker,
                            target,
                            point,
                            key: token,
                            rules: &manager.tokens,
                            settings,
                            now: clock.tick,
                        },
                        &mut template_points,
                        |key, radius| {
                            recipient_types.collect(member.root, [key], &mut addressed);
                            addressed.extend(recipient_names.iter().filter_map(
                                |(entity, name, world)| {
                                    (world.root == member.root
                                        && AssetId::from_key(&name.as_str().to_ascii_lowercase())
                                            == key)
                                        .then_some(entity)
                                },
                            ));
                            let mut passing: Vec<_> = addressed
                                .iter()
                                .filter_map(|&recipient| {
                                    let (pose, world, id) = recipients.get(recipient).ok()?;
                                    (world.root == member.root
                                        && radius.is_none_or(|radius| {
                                            pose.translation().xz().distance_squared(origin)
                                                <= radius * radius
                                        }))
                                    .then_some((id.map_or(u64::MAX, |id| id.0), recipient))
                                })
                                .collect();
                            passing.sort_unstable();
                            passing.dedup();
                            passing
                                .into_iter()
                                .map(|(_, recipient)| recipient)
                                .collect()
                        },
                        || random.next_u15(),
                        &mut commands,
                    )
                })
        } else {
            None
        };
        if let Some((delivered, name)) = delivered {
            commands
                .entity(job)
                .insert(super::staff_request_token_delivery::StaffRequestExecutionToken(delivered));
            task = find_highest_priority_compatible_staff_behavior_task(
                behavior_declarations,
                &token_tasks,
                catalogue,
                role,
                name,
                facts,
            );
        }
        if let Some(task) = task.filter(|_| !authored || delivered.is_some()) {
            commands.entity(job).insert(StaffJobBehaviorTask {
                execution_id: None,
                document: task.document,
                task: task.definition.id,
                declaration_index: task.declaration_index,
            });
        }
    }
    let mut attempts = jobs.p1();
    for (job, worker) in mismatches {
        let Ok(previous) = attempts.get_mut(job) else {
            continue;
        };
        if let Some(mut previous) = previous {
            previous.record(worker);
        } else {
            let mut previous = UnsuccessfulStaffJobCandidates::default();
            previous.record(worker);
            commands.entity(job).insert(previous);
        }
    }
}
