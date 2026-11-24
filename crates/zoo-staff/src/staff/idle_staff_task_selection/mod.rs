//! Staff without a job choose among the authored tasks they may start on
//! their own, scored by their authored needs, as animals and guests do.

use openzt2_zoo_state::behavior_task_execution_types::allocate_behavior_execution_id;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;

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
    world_definitions::staff_management::StaffRoleDefinition,
    AssetId,
};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorExitTankGate, RunningBehaviorInstruction,
};
use openzt2_zoo_state::topology::habitat_boundary_queries::HabitatBoundaryQueries;

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
    BehaviorSelfToken, ClaimedBehaviorSelfToken,
};
use openzt2_zoo_state::animal_behavior::behavior_target_retry_delays::BehaviorTargetRejections;
use openzt2_zoo_state::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore;
use openzt2_zoo_state::aquatic::aquatic_simulation_types::Tank;
use openzt2_zoo_state::behavior_task_execution_types::{
    abandon_behavior_task_for_reconsideration,
    BehaviorTaskExecutionPhase,
    BehaviorTaskGeneration, BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    SynchronizedBehaviorParticipant,
};
use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_world_position;
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::locomotion::locomotion_types::{
    Arrived, Destination, LocomotionMode, NavAgent, NavigateTo, NavigationFailed,
    NavigationRequestGeneration,
};
use openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

use super::{
    staff_employment_types::{Staff, StaffRole},
    staff_job_eligibility::staff_subject_matches,
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
    mut behavior_generations: Query<&mut BehaviorTaskGeneration>,
    target_retry_delays: BehaviorTargetRejections,
    surfaces: DrySurfaceEligibility,
    biomes: BehaviorTargetBiome,
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
            for target in idle_task_targets(task, staff, world, &world_objects, |object| {
                catalogue.find_object(object)
            }) {
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
                    position.xz().distance(target_position.xz()),
                    || biomes.at(world.root, target_position),
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
                );
            }
        }
        let Some(((document, declaration, task, target), score)) =
            ranking.finish(|| random.next_u32())
        else {
            continue;
        };
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

#[derive(Component)]
pub(super) struct PendingStaffExitTankGate {
    instruction:
        openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorInstructionId,
    request_id: u64,
}

/// Chooses an outside endpoint across an actual tank gate. The navigation
/// owner approaches and traverses that gate before completing this request.
pub(super) fn start_staff_exit_tank_gate_actions(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    roots: Query<(&ZooClock, &HabitatIndex, &TopologyGrid)>,
    tanks: Query<(), With<Tank>>,
    boundaries: HabitatBoundaryQueries,
    gates: Query<(
        openzt2_zoo_state::topology::topology_graph_types::FenceSegment,
        &openzt2_zoo_state::topology::gate_operation_types::Gate,
    )>,
    nodes: Query<&openzt2_zoo_state::topology::topology_graph_types::TopologyNode>,
    surfaces: DrySurfaceEligibility,
    mut navigation_generations: Query<&mut NavigationRequestGeneration>,
    mut requests: MessageWriter<NavigateTo>,
    mut actors: Query<
        (
            Entity,
            &GlobalTransform,
            &BehaviorTaskExecutionState,
            &WorldMember,
            &RunningBehaviorInstruction,
            Option<&mut PendingStaffExitTankGate>,
        ),
        (
            With<RunBehaviorExitTankGate>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
) {
    for (actor, transform, task, member, instruction, exiting) in
        behavior_action_actors(&mut actors, activation.0)
    {
        let Ok((clock, habitats, topology)) = roots.get(member.root) else {
            continue;
        };
        if !instruction.is_current(task)
            || (activation.0.is_none() && !instruction.can_update_after_activation(clock.tick))
            || task.next_action_tick > clock.tick
            || exiting
                .as_ref()
                .is_some_and(|exiting| exiting.instruction == instruction.id())
        {
            continue;
        }
        let tank =
            locate_habitat_at_world_position(habitats, transform.translation().xz(), *topology)
                .filter(|habitat| tanks.contains(*habitat));
        let Some(tank) = tank else {
            instruction.complete(actor, clock.tick, &mut commands);
            continue;
        };
        let endpoint = boundaries
            .fences(tank)
            .filter_map(|gate| {
                let (edge, gate) = gates.get(gate).ok()?;
                if gate.locked {
                    return None;
                }
                let a = topology.cell_translation(nodes.get(edge.a()).ok()?.cell);
                let b = topology.cell_translation(nodes.get(edge.b()).ok()?.cell);
                let middle = (a + b) * 0.5;
                let tangent = (b - a).xz().normalize_or_zero();
                let normal = Vec2::new(-tangent.y, tangent.x) * topology.spacing_m;
                [-1.0, 1.0].into_iter().find_map(|sign| {
                    let mut point = middle + Vec3::new(normal.x, 0.0, normal.y) * sign;
                    if locate_habitat_at_world_position(habitats, point.xz(), *topology)
                        == Some(tank)
                    {
                        return None;
                    }
                    point.y = surfaces
                        .water_surface_height(member.root, point.xz())
                        .or_else(|| surfaces.ground_height(member.root, point.xz()))?;
                    Some(point)
                })
            })
            .min_by(|left, right| {
                left.distance_squared(transform.translation())
                    .total_cmp(&right.distance_squared(transform.translation()))
            });
        let Some(destination) = endpoint else {
            openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let Ok(mut generation) = navigation_generations.get_mut(actor) else {
            continue;
        };
        let request_id = generation.advance();
        requests.write(NavigateTo {
            entity: actor,
            request_id,
            destination,
            arrival_radius_m: 0.1,
        });
        let pending = PendingStaffExitTankGate {
            instruction: instruction.id(),
            request_id,
        };
        if let Some(mut exiting) = exiting {
            *exiting = pending;
        } else {
            commands.entity(actor).insert(pending);
        }
    }
}

pub(super) fn retire_inactive_staff_tank_exits(
    mut commands: Commands,
    pending: Query<(
        Entity,
        &PendingStaffExitTankGate,
        Option<(
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorExitTankGate,
        )>,
        Has<PendingBehaviorTaskFailure>,
    )>,
) {
    for (actor, exit, instruction, failed) in &pending {
        if failed
            || instruction.is_none_or(|(task, instruction, _)| {
                exit.instruction != instruction.id() || !instruction.is_current(task)
            })
        {
            commands.entity(actor).remove::<PendingStaffExitTankGate>();
        }
    }
}

pub(super) fn settle_staff_tank_exit_navigation(
    mut commands: Commands,
    mut arrivals: MessageReader<Arrived>,
    mut failures: MessageReader<NavigationFailed>,
    roots: Query<(&ZooClock, &HabitatIndex, &TopologyGrid)>,
    tanks: Query<(), With<Tank>>,
    pending: Query<
        (
            &PendingStaffExitTankGate,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &GlobalTransform,
            &WorldMember,
        ),
        (
            With<RunBehaviorExitTankGate>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
) {
    let mut settled = bevy::ecs::entity::EntityHashSet::default();
    let outcomes = failures
        .read()
        .map(|event| (event.entity, event.request_id, true))
        .chain(
            arrivals
                .read()
                .map(|event| (event.entity, event.request_id, false)),
        );
    for (actor, request_id, failed) in outcomes {
        let Ok((exit, task, instruction, transform, member)) = pending.get(actor) else {
            continue;
        };
        if exit.request_id != request_id
            || exit.instruction != instruction.id()
            || !instruction.is_current(task)
            || settled.contains(&actor)
        {
            continue;
        }
        let Ok((clock, habitats, topology)) = roots.get(member.root) else {
            continue;
        };
        if !instruction.can_update_after_activation(clock.tick)
            || task.next_action_tick > clock.tick
        {
            continue;
        }
        let inside_a_tank =
            locate_habitat_at_world_position(habitats, transform.translation().xz(), *topology)
                .is_some_and(|habitat| tanks.contains(habitat));
        if failed || !inside_a_tank {
            settled.insert(actor);
            commands.entity(actor).remove::<PendingStaffExitTankGate>();
            if failed {
                openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            } else {
                instruction.complete(actor, clock.tick, &mut commands);
            }
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
    tokens: Query<(Entity, &BehaviorSelfToken, &ChildOf), Without<ClaimedBehaviorSelfToken>>,
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
    for (token_entity, token, holder) in &tokens {
        let member = holder.parent();
        let (Ok((role, transform, mode, world)), Some(payload)) =
            (staff.get(member), token.payload)
        else {
            continue;
        };
        let Ok(clock) = clocks.get(world.root) else {
            continue;
        };
        if clock.tick < token.available_at
            || clock.tick >= token.expires_at
            || started.contains(&member)
        {
            continue;
        }
        let Some(role) = catalogue.find_staff(role.0) else {
            continue;
        };
        let position = transform.translation();
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
                    candidate_role == BehaviorCandidateRole::Subject
                        && satisfies_idle_staff_fact(
                            world.root,
                            fact,
                            position,
                            position,
                            mode == Some(&LocomotionMode::Swim),
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
        debug!(staff = ?member, task = %task.name, animal = ?payload, "staff started a token task");
        commands
            .entity(token_entity)
            .insert(ClaimedBehaviorSelfToken { execution_id });
        commands.entity(member).insert((
            BehaviorTaskEffectiveScore {
                execution_id,
                template_score: Some(task.priority.unwrap_or_default()),
                linked_score: None,
                selected_candidate_score: None,
            },
            BehaviorTaskExecutionState {
                execution_id,
                program: task.id,
                target: Some(payload),
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
