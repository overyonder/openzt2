//! Staff without a job choose among the authored tasks they may start on
//! their own, scored by their authored needs, as animals and guests do.

use openzt2_zoo_state::behavior_task_execution_types::allocate_behavior_execution_id;

use bevy::prelude::*;
use openzt2_game_data::{
    behavior::{
        document::BehaviorTask,
        eligibility::{
            BehaviorCandidateEligibilityFact, BehaviorCandidateRole,
            BehaviorEligibilityFactComparison, BehaviorEligibilityFactInput,
            BehaviorEligibilityFactValue, BehaviorEntityStateEligibilityFact,
            BehaviorSpatialEligibilityFact,
        },
    },
    world_definitions::staff_management::StaffRoleKind,
    AssetId,
};

use openzt2_game_assets::behavior::behavior_asset_types::{
    BehaviorDocumentAsset, LoadedBehaviorDocumentCollection,
};
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use openzt2_zoo_state::animal_behavior::behavior_candidate_selection::{
    BehaviorCandidateRanking, BehaviorTargetBiome, candidate_requirements_are_satisfied,
    score_supported_behavior_task,
};
use openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;
use openzt2_zoo_state::animal_behavior::behavior_token_types::{
    BehaviorSelfToken, BehaviorTokenContext, ClaimedBehaviorSelfToken,
};
use openzt2_zoo_state::animal_behavior::behavior_target_retry_delays::BehaviorTargetRejections;
use openzt2_zoo_state::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore;
use openzt2_zoo_state::behavior_task_execution_types::{
    abandon_behavior_task_for_reconsideration,
    BehaviorTaskExecutionPhase,
    BehaviorTaskGeneration, BehaviorTaskExecutionState,
    SynchronizedBehaviorParticipant,
};
use openzt2_zoo_state::locomotion::locomotion_types::{Destination, LocomotionMode, NavAgent};
use openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility;

use super::staff_cognitive_state::{FossilStaffSensor, StaffNeeds};
use openzt2_zoo_state::extinct_animals::fossil_recovery_types::FossilSiteKind;

use super::{
    staff_employment_types::{Staff, StaffRole},
    staff_job_eligibility::staff_subject_matches,
    staff_job_types::{CurrentJob, StaffJobBehaviorTask},
};

/// The task an idle employee chose for itself. A claimed job replaces it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IdleStaffTask {
    execution_id: u64,
    fossil_site: Option<Entity>,
}

pub(crate) fn start_authored_idle_staff_tasks(
    mut commands: Commands,
    documents: Res<LoadedBehaviorDocumentCollection>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut behavior_generations: Query<&mut BehaviorTaskGeneration>,
    target_retry_delays: BehaviorTargetRejections,
    surfaces: DrySurfaceEligibility,
    biomes: BehaviorTargetBiome,
    clocks: Query<&ZooClock>,
    puzzle_sites: Query<(Entity, &FossilSiteKind, &WorldMember)>,
    running_staff: Query<&BehaviorTaskExecutionState, With<Staff>>,
    world_objects: Query<(Entity, &GlobalTransform, &DefinitionId, &WorldMember)>,
    mut idle: Query<
        (
            Entity,
            &StaffRole,
            &GlobalTransform,
            &WorldMember,
            Option<&LocomotionMode>,
            &mut BehaviorRandomStream,
            &StaffNeeds,
            &mut FossilStaffSensor,
        ),
        (
            With<Staff>,
            With<NavAgent>,
            Without<CurrentJob>,
            Without<BehaviorTaskExecutionState>,
            Without<Destination>,
            Without<SynchronizedBehaviorParticipant>,
        ),
    >,
) {
    if idle.is_empty() {
        return;
    }
    let (Some(view), Some(catalogue)) = (
        documents.create_declaration_index_view(&assets),
        active_definitions.get(&definitions),
    ) else {
        return;
    };
    let Some(policy) = catalogue.behavior_selection_policy() else {
        return;
    };
    if !policy.enabled {
        return;
    }
    let find_reservation = AssetId::from_key("find");
    let fossil_type = AssetId::from_key("fossilsite");
    let mut reserved_puzzles: bevy::ecs::entity::EntityHashSet = running_staff
        .iter()
        .filter_map(|task| {
            let declaration = assets
                .get(&task.document)?
                .behavior_task_at_index(task.declaration)?;
            if declaration.reservation_tag == Some(find_reservation) {
                task.target()
            } else {
                None
            }
        })
        .collect();
    for (staff, role, transform, world, mode, mut random, needs, mut fossil_sensor) in &mut idle {
        let Some(role) = catalogue.find_staff(role.0) else {
            continue;
        };
        let active_sites: Vec<Entity> = if role.role == StaffRoleKind::Paleontologist {
            puzzle_sites
                .iter()
                .filter_map(|(site, kind, member)| {
                    (*kind == FossilSiteKind::Puzzle && member.root == world.root).then_some(site)
                })
                .collect()
        } else {
            Vec::new()
        };
        let scan_puzzles = role.role == StaffRoleKind::Paleontologist
            && clocks
                .get(world.root)
                .ok()
                .zip(catalogue.simulation_timestep())
                .zip(catalogue.staff_manager())
                .is_some_and(|((clock, step), policy)| {
                    fossil_sensor.scan(
                        clock.tick as f64 * step.as_secs_f64(),
                        f64::from(policy.paleontologist_search_delay_seconds),
                        &active_sites,
                        |site| puzzle_sites.contains(site),
                    )
                });
        let position = transform.translation();
        let subject_matches = |required| staff_subject_matches(catalogue, role, required);
        let mut ranking = BehaviorCandidateRanking::new(policy.minimum_score);
        for (document, declaration, task) in view.behavior_tasks() {
            // Token tasks start from their delivered token, not from idleness.
            if !task.objects.is_empty() || task.execution.lowered_actions().is_empty() {
                continue;
            }
            let subject_can_start = task
                .candidate_eligibility_requirements
                .iter()
                .filter(|requirement| requirement.candidate_role == BehaviorCandidateRole::Subject)
                .all(|requirement| {
                    candidate_requirements_are_satisfied(
                        std::slice::from_ref(requirement),
                        |_, required| subject_matches(required),
                        |_, fact| {
                            if fact.fact_input
                                == BehaviorEligibilityFactInput::State(
                                    BehaviorEntityStateEligibilityFact::Teleportable,
                                )
                            {
                                return fact.expected_value
                                    == BehaviorEligibilityFactValue::Q16(
                                        i32::from(fossil_sensor.teleport) << 16,
                                    );
                            }
                            satisfies_idle_staff_fact(
                                world.root,
                                fact,
                                position,
                                position,
                                mode == Some(&LocomotionMode::Swim),
                                &surfaces,
                            )
                        },
                    )
                });
            if !subject_can_start {
                continue;
            }
            let find_fossil = task.reservation_tag == Some(find_reservation)
                && task
                    .targets
                    .iter()
                    .any(|target| AssetId::from_key(target) == fossil_type);
            let mut targets = if find_fossil {
                if !scan_puzzles {
                    continue;
                }
                active_sites
                    .iter()
                    .copied()
                    .filter(|&site| {
                        fossil_sensor.accepts(site) && !reserved_puzzles.contains(&site)
                    })
                    .filter_map(|site| {
                        world_objects
                            .get(site)
                            .ok()
                            .map(|(_, pose, ..)| Some((site, pose.translation())))
                    })
                    .collect::<Vec<_>>()
            } else {
                idle_task_targets(task, staff, world, &world_objects, |object| {
                    catalogue.find_object(object)
                })
            };
            if !find_fossil {
                targets.retain(|target| {
                    target.is_none_or(|(entity, _)| !puzzle_sites.contains(entity))
                });
            }
            for target in targets {
                if target.is_some_and(|(target, _)| target_retry_delays.rejects(staff, target)) {
                    continue;
                }
                let target_position = target.map_or(position, |(_, position)| position);
                let satisfied = candidate_requirements_are_satisfied(
                    &task.candidate_eligibility_requirements,
                    |candidate_role, required| match candidate_role {
                        BehaviorCandidateRole::Subject => true,
                        BehaviorCandidateRole::Target => match target {
                            None => required == AssetId::from_key("self"),
                            Some((object, _)) => {
                                world_objects
                                    .get(object)
                                    .is_ok_and(|(_, _, definition, _)| {
                                        required == definition.0
                                            || catalogue.find_object(definition.0).is_some_and(
                                                |object| {
                                                    object
                                                        .behavior_type_identifiers
                                                        .contains(&required)
                                                },
                                            )
                                    })
                            }
                        },
                        BehaviorCandidateRole::Object => false,
                    },
                    |candidate_role, fact| match candidate_role {
                        BehaviorCandidateRole::Subject => true,
                        BehaviorCandidateRole::Target => satisfies_idle_staff_fact(
                            world.root,
                            fact,
                            target_position,
                            position,
                            false,
                            &surfaces,
                        ),
                        BehaviorCandidateRole::Object => false,
                    },
                );
                if !satisfied {
                    continue;
                }
                let Some(score) = score_supported_behavior_task(
                    task,
                    0.0,
                    position.distance(target_position),
                    || biomes.at(world.root, target_position),
                    |attribute| needs.source_need(attribute),
                    |value| value.sample_q16(|upper| random.range_u32(upper).unwrap_or(0)),
                ) else {
                    continue;
                };
                ranking.consider(
                    (
                        document,
                        declaration,
                        task,
                        target.map(|(object, _)| object),
                        find_fossil,
                    ),
                    task.priority.unwrap_or_default(),
                    score,
                );
            }
        }
        let Some(((document, declaration, task, target, find_fossil), score)) =
            ranking.finish(|| random.next_u32())
        else {
            continue;
        };
        let fossil_site = target.filter(|_| find_fossil);
        if let Some(site) = fossil_site {
            reserved_puzzles.insert(site);
        }
        let execution_id = allocate_behavior_execution_id(
            staff,
            behavior_generations.get_mut(staff).ok(),
            &mut commands,
        );
        commands.entity(staff).insert((
            BehaviorTaskEffectiveScore {
                execution_id,
                template_score: Some(task.priority.unwrap_or_default()),
                linked_score: None,
                selected_candidate_score: Some(score),
            },
            BehaviorTaskExecutionState {
                execution_id,
                program: task.id,
                target: Some(target.unwrap_or(staff)),
                document: document.clone(),
                declaration,
                phase: BehaviorTaskExecutionPhase::Execution,
                action: 0,
                repetitions: 0,
                next_action_tick: 0,
                object: None,
                interaction_slot: None,
                depth: 0,
            },
            IdleStaffTask {
                execution_id,
                fossil_site,
            },
        ));
    }
}

/// `None` is the subject itself; other candidates are world objects in the
/// employee's world whose definition or authored types a target names.
fn idle_task_targets<'a>(
    task: &BehaviorTask,
    staff: Entity,
    world: &WorldMember,
    world_objects: &'a Query<(Entity, &GlobalTransform, &DefinitionId, &WorldMember)>,
    find_object: impl Fn(
        AssetId,
    ) -> Option<
        &'a openzt2_game_data::world_definitions::world_objects::WorldObjectDefinition,
    >,
) -> Vec<Option<(Entity, Vec3)>> {
    let targets = task
        .candidate_eligibility_requirements
        .iter()
        .filter(|requirement| requirement.candidate_role == BehaviorCandidateRole::Target)
        .flat_map(|requirement| requirement.candidate_type_identifiers.iter().copied())
        .collect::<Vec<_>>();
    if targets.is_empty() || targets.contains(&AssetId::from_key("self")) {
        return vec![None];
    }
    world_objects
        .iter()
        .filter(|(entity, _, definition, member)| {
            *entity != staff
                && member.root == world.root
                && targets.iter().any(|required| {
                    *required == definition.0
                        || find_object(definition.0).is_some_and(|object| {
                            object.behavior_type_identifiers.contains(required)
                        })
                })
        })
        .map(|(entity, transform, _, _)| Some((entity, transform.translation())))
        .collect()
}

/// Qualifiers an idle employee's own tasks use. Any other qualifier refuses
/// the candidate rather than passing unevaluated.
fn satisfies_idle_staff_fact(
    root: Entity,
    fact: &BehaviorCandidateEligibilityFact,
    fact_position: Vec3,
    subject_position: Vec3,
    swimming: bool,
    surfaces: &DrySurfaceEligibility,
) -> bool {
    let BehaviorEligibilityFactValue::Q16(expected) = fact.expected_value else {
        return false;
    };
    let actual = match fact.fact_input {
        BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::SupportedSurface) => {
            1 << 16
        }
        BehaviorEligibilityFactInput::Spatial(
            kind @ (BehaviorSpatialEligibilityFact::InSight
            | BehaviorSpatialEligibilityFact::NotInSight),
        ) if fact.comparison == BehaviorEligibilityFactComparison::Less => {
            let radius = expected as f32 / 65_536.0;
            let inside =
                subject_position.xz().distance_squared(fact_position.xz()) < radius * radius;
            return inside == (kind == BehaviorSpatialEligibilityFact::InSight);
        }
        BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::OnLand) => {
            let Some(on_land) = surfaces.on_land(root, fact_position.xz()) else {
                return false;
            };
            i32::from(on_land) << 16
        }
        BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::InWater) => {
            let Some(on_land) = surfaces.on_land(root, fact_position.xz()) else {
                return false;
            };
            i32::from(!on_land) << 16
        }
        BehaviorEligibilityFactInput::State(BehaviorEntityStateEligibilityFact::Swimming) => {
            i32::from(swimming) << 16
        }
        _ => return false,
    };
    match fact.comparison {
        BehaviorEligibilityFactComparison::Equal => actual == expected,
        BehaviorEligibilityFactComparison::NotEqual => actual != expected,
        BehaviorEligibilityFactComparison::Less => actual < expected,
        BehaviorEligibilityFactComparison::LessOrEqual => actual <= expected,
        BehaviorEligibilityFactComparison::Greater => actual > expected,
        BehaviorEligibilityFactComparison::GreaterOrEqual => actual >= expected,
    }
}

pub(super) fn collect_completed_staff_fossil_sites(
    mut completed: MessageReader<
        openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskFinished,
    >,
    mut staff: Query<(&IdleStaffTask, &mut FossilStaffSensor)>,
    sites: Query<(), With<FossilSiteKind>>,
) {
    for completion in completed.read() {
        let Ok((task, mut sensor)) = staff.get_mut(completion.actor) else {
            continue;
        };
        if task.execution_id == completion.execution_id {
            if let Some(site) = task.fossil_site.filter(|&site| sites.contains(site)) {
                sensor.collect(site);
            }
        }
    }
}

/// A claimed job replaces the employee's idle task at once, without its
/// failure branch, so the job's own task starts next.
pub(crate) fn interrupt_idle_staff_tasks_for_claimed_jobs(
    mut commands: Commands,
    staff: Query<(
        Entity,
        &CurrentJob,
        &IdleStaffTask,
        Option<&BehaviorTaskExecutionState>,
    )>,
    jobs: Query<&StaffJobBehaviorTask>,
) {
    for (employee, current_job, idle, task) in &staff {
        let still_idle = task.is_some_and(|task| task.execution_id == idle.execution_id);
        let job_unstarted = jobs
            .get(current_job.job())
            .is_ok_and(|job| job.execution_id.is_none());
        if still_idle && job_unstarted {
            abandon_behavior_task_for_reconsideration(employee, &mut commands);
        }
        if !still_idle || job_unstarted {
            commands.entity(employee).remove::<IdleStaffTask>();
        }
    }
}

/// Starts a staff task its delivered token calls for, such as `Scared_Staff`
/// when an attacking animal gives nearby staff `t_Attack`. The token's payload,
/// the animal, is the `fromToken` target.
// Only staff without a job or task react; a frightened keeper on a
// job finishes it first.
#[allow(clippy::too_many_arguments)]
pub(crate) fn start_staff_token_tasks(
    mut commands: Commands,
    documents: Res<LoadedBehaviorDocumentCollection>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    clocks: Query<&ZooClock>,
    mut behavior_generations: Query<&mut BehaviorTaskGeneration>,
    surfaces: DrySurfaceEligibility,
    token_tasks: Res<crate::staff::staff_job_eligibility::StaffTasksByRequestToken>,
    tokens: Query<
        (
            Entity,
            &BehaviorSelfToken,
            &ChildOf,
            Option<&BehaviorTokenContext>,
        ),
        Without<ClaimedBehaviorSelfToken>,
    >,
    targets: Query<(&WorldMember, &GlobalTransform, Option<&LocomotionMode>)>,
    staff: Query<
        (
            &StaffRole,
            &GlobalTransform,
            Option<&LocomotionMode>,
            &WorldMember,
        ),
        (
            With<Staff>,
            With<NavAgent>,
            Without<CurrentJob>,
            Without<BehaviorTaskExecutionState>,
            Without<SynchronizedBehaviorParticipant>,
        ),
    >,
) {
    if tokens.is_empty() {
        return;
    }
    let (Some(view), Some(catalogue)) = (
        documents.create_declaration_index_view(&assets),
        active_definitions.get(&definitions),
    ) else {
        return;
    };
    let mut started = Vec::new();
    for (token_entity, token, holder, context) in &tokens {
        let member = holder.parent();
        let Ok((role, transform, mode, world)) = staff.get(member) else {
            continue;
        };
        let Ok(clock) = clocks.get(world.root) else {
            continue;
        };
        if !token.is_available_at(clock.tick) || started.contains(&member) {
            continue;
        }
        let Some(role) = catalogue.find_staff(role.0) else {
            continue;
        };
        let position = transform.translation();
        let (target_position, target_swimming) = if let Some(payload) = token.payload {
            let Ok((target_world, target_transform, target_mode)) = targets.get(payload) else {
                continue;
            };
            if target_world.root != world.root {
                continue;
            }
            (
                target_transform.translation(),
                target_mode == Some(&LocomotionMode::Swim),
            )
        } else if let Some(point) = context.and_then(|context| context.point) {
            (point, false)
        } else {
            continue;
        };
        let mut chosen = None;
        for (document, declaration, task) in token_tasks
            .candidates(token.name)
            .filter_map(|id| view.find_behavior_task_location(id))
        {
            if !task
                .targets
                .iter()
                .any(|target| target.eq_ignore_ascii_case("fromtoken"))
                || !(task.subjects.is_empty()
                    || task.subjects.iter().any(|subject| {
                        staff_subject_matches(
                            catalogue,
                            role,
                            AssetId::from_key(&subject.to_ascii_lowercase()),
                        )
                    }))
            {
                continue;
            }
            let satisfied = candidate_requirements_are_satisfied(
                &task.candidate_eligibility_requirements,
                |candidate_role, required| match candidate_role {
                    BehaviorCandidateRole::Subject => {
                        staff_subject_matches(catalogue, role, required)
                    }
                    BehaviorCandidateRole::Target => required == AssetId::from_key("fromtoken"),
                    BehaviorCandidateRole::Object => required == token.name,
                },
                |candidate_role, fact| {
                    let (fact_position, swimming) = match candidate_role {
                        BehaviorCandidateRole::Subject => {
                            (position, mode == Some(&LocomotionMode::Swim))
                        }
                        BehaviorCandidateRole::Target => (target_position, target_swimming),
                        BehaviorCandidateRole::Object => return false,
                    };
                    satisfies_idle_staff_fact(
                        world.root,
                        fact,
                        fact_position,
                        position,
                        swimming,
                        &surfaces,
                    )
                },
            );
            let priority = task.priority.unwrap_or_default();
            if satisfied
                && chosen
                    .as_ref()
                    .is_none_or(|(_, _, best): &(_, _, &BehaviorTask)| {
                        priority > best.priority.unwrap_or_default()
                    })
            {
                chosen = Some((document, declaration, task));
            }
        }
        let Some((document, declaration, task)) = chosen else {
            continue;
        };
        let execution_id = allocate_behavior_execution_id(
            member,
            behavior_generations.get_mut(member).ok(),
            &mut commands,
        );
        debug!(staff = ?member, task = %task.name, target = ?token.payload, "staff started a token task");
        commands
            .entity(token_entity)
            .insert(ClaimedBehaviorSelfToken { execution_id });
        commands.entity(member).insert((
            BehaviorTaskEffectiveScore {
                execution_id,
                template_score: Some(task.priority.unwrap_or_default()),
                linked_score: None,
                selected_candidate_score: context
                    .and_then(|context| context.force.then_some(1_000_000_000.0)),
            },
            BehaviorTaskExecutionState {
                execution_id,
                program: task.id,
                target: token.payload,
                document,
                declaration,
                phase: BehaviorTaskExecutionPhase::Execution,
                action: 0,
                repetitions: 0,
                next_action_tick: 0,
                object: Some(token_entity),
                interaction_slot: None,
                depth: 0,
            },
        ));
        started.push(member);
    }
}
