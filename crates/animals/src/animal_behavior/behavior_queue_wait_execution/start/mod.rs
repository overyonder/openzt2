//! Admit once, then repeat the authored waiting set without renewing its deadline.

use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;
use super::super::behavior_container_admission::BehaviorAdmissionInstruction;
use super::super::{
    behavior_random_stream_state::BehaviorRandomStream,
    behavior_subject_type_resolution::{behavior_subject_type_identifiers, AnimalBehaviorSubjectTypes},
    behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
};
use super::{ActiveQueueWait, QueueWaitSources};
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDeclarationIndexView;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_game_data::behavior::action::queue::BehaviorQueueWaitAction;
use openzt2_game_data::AssetId;
use openzt2_zoo_state::animal_behavior::interaction_container_membership::requests::{
    InteractionContainerAdmissionOutcome, InteractionContainerAdmissionRequest,
};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorWaitInteractionQueue, RunningBehaviorInstruction, SelectedBehaviorSet,
};

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::animal_behavior::interaction_container_membership::InteractionContainerMemberships;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArchetype;
use openzt2_zoo_state::guests::guest_simulation_types::GuestRng;
use openzt2_zoo_state::staff::staff_employment_types::StaffRole;

#[derive(SystemParam)]
pub(super) struct QueueWaitActors<'w, 's> {
    tasks: Query<
        'w,
        's,
        (
            Entity,
            (
                &'static mut BehaviorTaskExecutionState,
                &'static RunningBehaviorInstruction,
                &'static RunBehaviorWaitInteractionQueue,
            ),
            Option<&'static ActiveQueueWait>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    identities: Query<
        'w,
        's,
        (
            Option<&'static SpeciesHandle>,
            Option<&'static StaffRole>,
            Option<&'static GuestArchetype>,
        ),
    >,
    random: Query<
        'w,
        's,
        (
            Option<&'static mut GuestRng>,
            Option<&'static mut BehaviorRandomStream>,
        ),
    >,
}

pub(super) fn start_or_repeat_authored_queue_wait(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
    sources: QueueWaitSources,
    animal_types: AnimalBehaviorSubjectTypes,
    mut actors: QueueWaitActors,
    memberships: InteractionContainerMemberships,
) {
    let Some(definitions) = sources.definitions.get(&sources.definition_assets) else {
        return;
    };
    let Some(declarations) = sources
        .declarations
        .create_declaration_index_view(&sources.behaviors)
    else {
        return;
    };
    for (actor, (task, instruction, running), active_wait) in
        behavior_action_actors(&mut actors.tasks, activation.0)
    {
        if activation.0.is_none() && !instruction.can_update_after_activation(clock.tick) {
            continue;
        }
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(&task) {
            continue;
        }
        let action = &running.0;
        let Some(target) = task.target() else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let Some(object) = sources
            .targets
            .get(target)
            .ok()
            .and_then(|id| definitions.find_object(id.0))
        else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let tag = action.container.or_else(|| instruction.reservation_tag());
        if memberships
            .actor_slot(target, actor)
            .and_then(|index| object.interaction_slots.get(index))
            .is_some_and(|slot| !slot.is_queue && tag.is_none_or(|tag| slot.reservation_tag == tag))
        {
            instruction.complete(actor, clock.tick, &mut commands);
            continue;
        }
        let Some((queue_index, queue)) = object
            .interaction_slots
            .iter()
            .enumerate()
            .find(|(_, slot)| slot.is_queue && tag.is_none_or(|tag| slot.reservation_tag == tag))
        else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        if active_wait.is_some_and(|wait| {
            wait.execution_id == task.execution_id
                && wait.target == target
                && wait.queue_slot == queue_index
                && task.is_at_exact_frame(&wait.origin, wait.stack_depth)
                && memberships.actor_slot(target, actor) == Some(queue_index)
        }) {
            let selection = resolve_queue_waiting_set(
                actor,
                &task,
                action.waiting_behavior_set,
                clock.tick,
                (definitions, declarations),
                &actors.identities,
                &animal_types,
            );
            if let Some(selection) = selection {
                instruction.enter_set(actor, selection, &mut commands);
            } else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            }
            continue;
        }
        let service_slots = object
            .interaction_slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| !slot.is_queue && slot.reservation_tag == queue.reservation_tag)
            .map(|(index, slot)| (index, slot.capacity))
            .collect();
        let request = InteractionContainerAdmissionRequest::Admit {
            container: target,
            member: actor,
            slots: vec![(queue_index, queue.capacity)],
            despawn_on_refusal: false,
            queue_service_slots: service_slots,
        };
        let instruction = BehaviorAdmissionInstruction::new(actor, &task, instruction);
        let action = action.clone();
        commands.queue(move |world: &mut World| {
            if instruction.is_current(world) {
                let outcome = request.commit(world);
                world.trigger(QueueWaitAdmitted {
                    actor,
                    target,
                    action,
                    outcome,
                });
            }
        });
    }
}

#[derive(EntityEvent)]
pub(super) struct QueueWaitAdmitted {
    #[event_target]
    actor: Entity,
    target: Entity,
    action: BehaviorQueueWaitAction,
    outcome: InteractionContainerAdmissionOutcome,
}

pub(super) fn apply_queue_wait_admission(
    admitted: On<QueueWaitAdmitted>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
    sources: QueueWaitSources,
    animal_types: AnimalBehaviorSubjectTypes,
    mut actors: QueueWaitActors,
) {
    let actor = admitted.actor;
    let target = admitted.target;
    let action = &admitted.action;
    let Ok((_, (mut task, instruction, _), active_wait)) = actors.tasks.get_mut(actor) else {
        return;
    };
    let InteractionContainerAdmissionOutcome::Granted {
        slot: queue_index,
        queue_service_available,
        ..
    } = admitted.outcome
    else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    if !instruction.is_current(&task) {
        return;
    }
    task.interaction_slot = Some(queue_index);
    if queue_service_available {
        commands.entity(actor).remove::<ActiveQueueWait>();
        instruction.complete(actor, clock.tick, &mut commands);
        return;
    }
    let (Some(definitions), Some(declarations)) = (
        sources.definitions.get(&sources.definition_assets),
        sources
            .declarations
            .create_declaration_index_view(&sources.behaviors),
    ) else {
        return;
    };
    let Some(selection) = resolve_queue_waiting_set(
        actor,
        &task,
        action.waiting_behavior_set,
        clock.tick,
        (definitions, declarations),
        &actors.identities,
        &animal_types,
    ) else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    let origin = task.create_return_frame_after_action(task.action);
    if active_wait.is_none() {
        let duration = if action.minimum_wait == action.maximum_wait {
            action.minimum_wait
        } else {
            let Ok((guest_random, behavior_random)) = actors.random.get_mut(actor) else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                return;
            };
            let mut guest_random = guest_random;
            let mut behavior_random = behavior_random;
            let Some(random) = guest_random
                .as_deref_mut()
                .map(|rng| &mut rng.0)
                .or_else(|| behavior_random.as_deref_mut().map(|rng| &mut rng.0))
            else {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                return;
            };
            action.minimum_wait
                + (action.maximum_wait - action.minimum_wait).mul_f64(f64::from(random.unit_f32()))
        };
        if duration.is_zero() {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            return;
        }
        let ticks = duration.as_nanos().div_ceil(
            definitions
                .simulation_timestep()
                .expect("simulation timing is loaded")
                .as_nanos(),
        );
        commands.entity(actor).insert(ActiveQueueWait {
            execution_id: task.execution_id,
            origin: origin.clone(),
            stack_depth: task.depth,
            target,
            queue_slot: queue_index,
            deadline_tick: clock
                .tick
                .saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX)),
        });
    }
    instruction.enter_set(actor, selection, &mut commands);
}

fn resolve_queue_waiting_set(
    actor: Entity,
    task: &BehaviorTaskExecutionState,
    program: AssetId,
    tick: u64,
    (definitions, declarations): (WorldDefinitionsView<'_>, BehaviorDeclarationIndexView<'_>),
    identities: &Query<(
        Option<&SpeciesHandle>,
        Option<&StaffRole>,
        Option<&GuestArchetype>,
    )>,
    animal_types: &AnimalBehaviorSubjectTypes,
) -> Option<SelectedBehaviorSet> {
    let (species, staff, guest) = identities.get(actor).ok()?;
    let (document, declaration) = declarations.find_behavior_set_location(
        program,
        behavior_subject_type_identifiers(
            species,
            staff,
            guest,
            definitions,
            animal_types.borrow_for(actor),
        ),
    )?;
    Some(SelectedBehaviorSet {
        program,
        document,
        declaration,
        tick,
        repeat: Some(task.repetitions),
        target: None,
    })
}
