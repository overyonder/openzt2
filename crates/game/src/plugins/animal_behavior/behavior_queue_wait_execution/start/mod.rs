//! Admit once, then repeat the authored waiting set without renewing its deadline.

use super::super::{
    behavior_random_stream_state::BehaviorRandomStream,
    behavior_subject_type_resolution::behavior_subject_type_identifiers,
    behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
    interaction_container_occupancy::InteractionContainerOccupancy,
};
use super::{ActiveQueueWait, QueueWaitSources};
use crate::plugins::{
    animal_lifecycle::types::SpeciesHandle,
    behavior_task_execution_types::{
        advance_behavior_task_to_next_action, find_current_behavior_task_action,
        BehaviorTaskExecutionPhase, BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
    },
    guests::guest_simulation_types::{GuestArchetype, GuestRng},
    simulation_time::simulation_clock_types::ZooClock,
    staff::staff_employment_types::StaffRole,
};
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::behavior::action_record::BehaviorAction;

#[derive(SystemParam)]
pub(super) struct QueueWaitActors<'w, 's> {
    tasks: Query<
        'w,
        's,
        (
            Entity,
            &'static mut BehaviorTaskExecutionState,
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
    mut commands: Commands,
    clock: Res<ZooClock>,
    sources: QueueWaitSources,
    mut actors: QueueWaitActors,
    mut occupancy: ResMut<InteractionContainerOccupancy>,
) {
    let (Some(definitions), Some(declarations)) = (
        sources.definitions.get(&sources.definition_assets),
        sources
            .declarations
            .create_declaration_index_view(&sources.behaviors),
    ) else {
        return;
    };
    for (actor, mut task, active_wait) in &mut actors.tasks {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::WaitInteractionQueue(action)) = sources
            .behaviors
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
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
        let tag = action
            .container
            .or_else(|| task.reservation_tag(&sources.behaviors));
        if occupancy
            .actor_slot(target, actor)
            .and_then(|index| object.interaction_slots.get(index))
            .is_some_and(|slot| !slot.is_queue && tag.is_none_or(|tag| slot.reservation_tag == tag))
        {
            advance_behavior_task_to_next_action(&mut task, clock.tick);
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
        if !occupancy.admit(target, actor, queue_index, queue.capacity) {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        task.interaction_slot = Some(queue_index);
        if occupancy.queue_front_can_enter_service(
            target,
            actor,
            queue_index,
            &object.interaction_slots,
        ) {
            advance_behavior_task_to_next_action(&mut task, clock.tick);
            commands.entity(actor).remove::<ActiveQueueWait>();
            continue;
        }
        let Ok((species, staff, guest)) = actors.identities.get(actor) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let Some((document, declaration)) = declarations.find_behavior_set_location(
            action.waiting_behavior_set,
            behavior_subject_type_identifiers(species, staff, guest, definitions),
        ) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let waiting_set = action.waiting_behavior_set;
        let origin = task.create_return_frame_after_action(task.action);
        if active_wait.is_none() {
            let duration = if action.minimum_wait == action.maximum_wait {
                action.minimum_wait
            } else {
                let Ok((guest_random, behavior_random)) = actors.random.get_mut(actor) else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                let mut guest_random = guest_random;
                let mut behavior_random = behavior_random;
                let Some(random) = guest_random
                    .as_deref_mut()
                    .map(|rng| &mut rng.0)
                    .or_else(|| behavior_random.as_deref_mut().map(|rng| &mut rng.0))
                else {
                    mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                    continue;
                };
                action.minimum_wait
                    + (action.maximum_wait - action.minimum_wait)
                        .mul_f64(f64::from(random.unit_f32()))
            };
            if duration.is_zero() {
                mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
                continue;
            }
            let ticks = duration
                .as_nanos()
                .saturating_mul(u128::from(definitions.timing().fixed_hz))
                .div_ceil(1_000_000_000);
            commands.entity(actor).insert(ActiveQueueWait {
                origin: origin.clone(),
                stack_depth: task.stack.len(),
                target,
                queue_slot: queue_index,
                deadline_tick: clock
                    .tick
                    .saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX)),
            });
        }
        if !task.push_return_frame(origin) {
            commands.entity(actor).remove::<ActiveQueueWait>();
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        task.program = waiting_set;
        task.document = document;
        task.declaration = declaration;
        task.phase = BehaviorTaskExecutionPhase::Set;
        task.action = 0;
        task.repetitions = 0;
        task.next_action_tick = clock.tick.saturating_add(1);
    }
}
