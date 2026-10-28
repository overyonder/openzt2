//! Staff without a job choose among the authored tasks they may start on
//! their own, scored by their authored needs, as animals and guests do.

use arrayvec::ArrayVec;
use bevy::prelude::*;
use openzt2_game_data::{
    behavior::{
        action_record::BehaviorAction,
        document::BehaviorTask,
        eligibility::{
            BehaviorCandidateEligibilityFact, BehaviorCandidateRole,
            BehaviorEligibilityFactComparison, BehaviorEligibilityFactInput,
            BehaviorEligibilityFactValue, BehaviorEntityStateEligibilityFact,
            BehaviorSpatialEligibilityFact,
        },
    },
    world_definitions::staff_management::StaffRoleDefinition,
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
    candidate_requirements_are_satisfied, score_supported_behavior_task,
    BehaviorCandidateRanking,
};
use openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;
use openzt2_zoo_state::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::Tank;
use openzt2_zoo_state::behavior_task_execution_types::{
    abandon_behavior_task_for_reconsideration, advance_behavior_task_to_next_action,
    find_current_behavior_task_action, BehaviorTaskExecutionPhase,
    BehaviorTaskExecutionSequence, BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    SynchronizedBehaviorParticipant,
};
use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_world_position;
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::locomotion::locomotion_types::{Destination, LocomotionMode, NavAgent};
use openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

use super::{
    staff_employment_types::{AvailableForWork, Staff, StaffRole},
    staff_job_eligibility::authored_staff_role_subject_names,
    staff_job_types::{CurrentJob, StaffJobBehaviorTask},
};

/// The task an idle employee chose for itself. A claimed job replaces it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IdleStaffTask {
    execution_id: u64,
}

pub(crate) fn start_authored_idle_staff_tasks(
    mut commands: Commands,
    documents: Res<LoadedBehaviorDocumentCollection>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut sequence: ResMut<BehaviorTaskExecutionSequence>,
    surfaces: DrySurfaceEligibility,
    world_objects: Query<(Entity, &GlobalTransform, &DefinitionId, &WorldMember)>,
    mut idle: Query<
        (
            Entity,
            &StaffRole,
            &GlobalTransform,
            &WorldMember,
            Option<&LocomotionMode>,
            &mut BehaviorRandomStream,
        ),
        (
            With<Staff>,
            With<AvailableForWork>,
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
    for (staff, role, transform, world, mode, mut random) in &mut idle {
        let Some(role) = catalogue.find_staff(role.0) else {
            continue;
        };
        let position = transform.translation();
        let subject_types = authored_staff_role_subject_names(role.role);
        let subject_matches = |required: AssetId| {
            required == AssetId::from_key("staff")
                || subject_types
                    .iter()
                    .any(|name| required == AssetId::from_key(name))
        };
        let mut ranking = BehaviorCandidateRanking::new(policy.minimum_score);
        for (document, declaration, task) in view.behavior_tasks() {
            // Token tasks start from their delivered token, not from idleness.
            if !task.objects.is_empty() || task.execution.lowered_actions().is_empty() {
                continue;
            }
            for target in idle_task_targets(task, staff, world, &world_objects, |object| {
                catalogue.find_object(object)
            }) {
                let target_position = target.map_or(position, |(_, position)| position);
                let satisfied = candidate_requirements_are_satisfied(
                    &task.candidate_eligibility_requirements,
                    |candidate_role, required| match candidate_role {
                        BehaviorCandidateRole::Subject => subject_matches(required),
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
                    |candidate_role, fact| {
                        let (fact_position, swimming) = match candidate_role {
                            BehaviorCandidateRole::Subject => {
                                (position, mode == Some(&LocomotionMode::Swim))
                            }
                            BehaviorCandidateRole::Target => (target_position, false),
                            BehaviorCandidateRole::Object => return false,
                        };
                        satisfies_idle_staff_fact(
                            fact,
                            fact_position,
                            position,
                            swimming,
                            &surfaces,
                        )
                    },
                );
                if !satisfied {
                    continue;
                }
                let Some(score) = score_supported_behavior_task(
                    task,
                    0.0,
                    position.xz().distance(target_position.xz()),
                    |attribute| staff_need(role, attribute),
                    |value| value.sample_q16(|_| 0),
                ) else {
                    continue;
                };
                ranking.consider(
                    (
                        document,
                        declaration,
                        task,
                        target.map(|(object, _)| object),
                    ),
                    task.priority.unwrap_or_default(),
                    score,
                    |upper| random.range_u32(upper),
                );
            }
        }
        let Some(((document, declaration, task, target), score)) = ranking.finish(false) else {
            continue;
        };
        let execution_id = sequence.next();
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
                stack: ArrayVec::new(),
            },
            IdleStaffTask { execution_id },
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

/// An authored need, deprived by its value and triggered above its threshold.
fn staff_need(role: &StaffRoleDefinition, attribute: AssetId) -> Option<(i32, bool)> {
    role.state_variables
        .iter()
        .find(|variable| variable.name == attribute)
        .map(|variable| {
            (
                (f64::from(variable.value) * 65_536.0).round() as i32,
                variable.value > variable.trigger_threshold,
            )
        })
}

/// Qualifiers an idle employee's own tasks use. Any other qualifier refuses
/// the candidate rather than passing unevaluated.
fn satisfies_idle_staff_fact(
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
            let Some(on_land) = surfaces.on_land(fact_position.xz()) else {
                return false;
            };
            i32::from(on_land) << 16
        }
        BehaviorEligibilityFactInput::Spatial(BehaviorSpatialEligibilityFact::InWater) => {
            let Some(on_land) = surfaces.on_land(fact_position.xz()) else {
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
            .get(current_job.job)
            .is_ok_and(|job| job.execution_id.is_none());
        if still_idle && job_unstarted {
            abandon_behavior_task_for_reconsideration(employee, &mut commands);
        }
        if !still_idle || job_unstarted {
            commands.entity(employee).remove::<IdleStaffTask>();
        }
    }
}

use openzt2_zoo_state::staff::staff_tank_gate_types::StaffExitTankGate;

/// Chooses an outside endpoint across an actual tank gate. The navigation
/// owner approaches and traverses that gate before completing this request.
pub(crate) fn execute_staff_exit_tank_gate_actions(
    mut commands: Commands,
    clock: Res<ZooClock>,
    documents: Res<Assets<BehaviorDocumentAsset>>,
    habitats: Res<HabitatIndex>,
    topology: Res<TopologyGrid>,
    tanks: Query<&openzt2_zoo_state::habitat::habitat_types::HabitatRegion, With<Tank>>,
    gates: Query<(
        &openzt2_zoo_state::topology::topology_graph_types::FenceEdge,
        &openzt2_zoo_state::topology::gate_operation_types::Gate,
    )>,
    nodes: Query<&openzt2_zoo_state::topology::topology_graph_types::TopologyNode>,
    surfaces: DrySurfaceEligibility,
    mut sequence: ResMut<
        openzt2_zoo_state::locomotion::locomotion_types::NavigationRequestSequence,
    >,
    mut requests: MessageWriter<openzt2_zoo_state::locomotion::locomotion_types::NavigateTo>,
    mut arrivals: MessageReader<openzt2_zoo_state::locomotion::locomotion_types::Arrived>,
    mut failures: MessageReader<openzt2_zoo_state::locomotion::locomotion_types::NavigationFailed>,
    mut actors: Query<
        (
            Entity,
            &GlobalTransform,
            &mut BehaviorTaskExecutionState,
            Option<&StaffExitTankGate>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    let arrived: Vec<_> = arrivals
        .read()
        .map(|event| (event.entity, event.request_id))
        .collect();
    let failed: Vec<_> = failures
        .read()
        .map(|event| (event.entity, event.request_id))
        .collect();
    for (actor, transform, mut task, exiting) in &mut actors {
        let exiting = exiting.filter(|exiting| {
            exiting.execution_id == task.execution_id
                && exiting.depth == task.stack.len()
                && task.is_at_or_nested_under(&exiting.origin, exiting.depth)
        });
        if exiting.is_none() {
            commands.entity(actor).remove::<StaffExitTankGate>();
        }
        if task.next_action_tick > clock.tick
            || !matches!(
                documents
                    .get(&task.document)
                    .and_then(|document| find_current_behavior_task_action(document, &task)),
                Some(BehaviorAction::ExitTankGate)
            )
        {
            continue;
        }
        let tank =
            locate_habitat_at_world_position(&habitats, transform.translation().xz(), *topology)
                .filter(|habitat| tanks.contains(*habitat));
        if let Some(exiting) = exiting.filter(|exiting| exiting.execution_id == task.execution_id) {
            if failed.contains(&(actor, exiting.request_id)) {
                commands.entity(actor).remove::<StaffExitTankGate>();
                openzt2_zoo_state::animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            } else if arrived.contains(&(actor, exiting.request_id)) && tank.is_none() {
                commands.entity(actor).remove::<StaffExitTankGate>();
                advance_behavior_task_to_next_action(&mut task, clock.tick);
            }
            continue;
        }
        let Some(tank) = tank else {
            advance_behavior_task_to_next_action(&mut task, clock.tick);
            continue;
        };
        let Ok(region) = tanks.get(tank) else {
            continue;
        };
        let endpoint = region
            .boundary_fence_entities
            .iter()
            .filter_map(|gate| {
                let (edge, gate) = gates.get(*gate).ok()?;
                if gate.locked {
                    return None;
                }
                let a = topology.cell_translation(nodes.get(edge.a).ok()?.cell);
                let b = topology.cell_translation(nodes.get(edge.b).ok()?.cell);
                let middle = (a + b) * 0.5;
                let tangent = (b - a).xz().normalize_or_zero();
                let normal = Vec2::new(-tangent.y, tangent.x) * topology.spacing_m;
                [-1.0, 1.0].into_iter().find_map(|sign| {
                    let mut point = middle + Vec3::new(normal.x, 0.0, normal.y) * sign;
                    if locate_habitat_at_world_position(&habitats, point.xz(), *topology)
                        == Some(tank)
                    {
                        return None;
                    }
                    point.y = surfaces
                        .water_surface_height(point.xz())
                        .or_else(|| surfaces.ground_height(point.xz()))?;
                    Some(point)
                })
            })
            .min_by(|left, right| {
                left.distance_squared(transform.translation())
                    .total_cmp(&right.distance_squared(transform.translation()))
            });
        let Some(destination) = endpoint else {
            openzt2_zoo_state::animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let request_id = sequence.next();
        requests.write(
            openzt2_zoo_state::locomotion::locomotion_types::NavigateTo {
                entity: actor,
                request_id,
                destination,
                arrival_radius_m: 0.1,
            },
        );
        commands.entity(actor).insert(StaffExitTankGate {
            execution_id: task.execution_id,
            request_id,
            origin: task.create_return_frame_after_action(task.action),
            depth: task.stack.len(),
        });
    }
}
