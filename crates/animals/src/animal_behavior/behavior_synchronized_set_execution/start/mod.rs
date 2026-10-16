use super::super::{
    behavior_subject_type_resolution::behavior_subject_type_identifiers,
    behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
};
use super::{
    SynchronizedBehaviorOwner, SynchronizedBehaviorParticipant, SynchronizedBehaviorSources,
};
use bevy::{ecs::system::SystemParam, platform::collections::HashSet, prelude::*};
use openzt2_game_data::behavior::action_record::BehaviorAction;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;

#[derive(SystemParam)]
pub(super) struct SynchronizedBehaviorActors<'w, 's> {
    tasks: Query<
        'w,
        's,
        (Entity, &'static mut BehaviorTaskExecutionState),
        (
            Without<SynchronizedBehaviorOwner>,
            Without<SynchronizedBehaviorParticipant>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
    occupied_targets: Query<
        'w,
        's,
        (),
        Or<(
            With<BehaviorTaskExecutionState>,
            With<SynchronizedBehaviorParticipant>,
        )>,
    >,
    controllers: Query<
        'w,
        's,
        (
            &'static AnimationPresentationOwner,
            &'static mut AnimationPlaybackController,
        ),
    >,
}

pub(super) fn start_synchronized_behavior_sets(
    mut commands: Commands,
    clock: Res<ZooClock>,
    sources: SynchronizedBehaviorSources,
    animal_types: super::super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    mut actors: SynchronizedBehaviorActors,
    mut claims_this_tick: Local<HashSet<Entity>>,
    mut execution_sequence: ResMut<
        openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionSequence,
    >,
) {
    claims_this_tick.clear();
    let (Some(definitions), Some(declarations)) = (
        sources.definitions.get(&sources.definition_assets),
        sources
            .declarations
            .create_declaration_index_view(&sources.documents),
    ) else {
        return;
    };
    for (actor, mut task) in &mut actors.tasks {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::SynchronizedSets(action)) = sources
            .documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
        else {
            continue;
        };
        let Some(target) = task.target().filter(|target| *target != actor) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        if actors.occupied_targets.contains(target) || claims_this_tick.contains(&target) {
            // Native non-interruptible/busy partners reject the synchronization.
            // Replacing an existing task requires its authored interrupt policy.
            warn!(
                ?actor,
                ?target,
                "synchronized partner is busy; active-task interrupt policy is not implemented"
            );
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        // An object partner, such as a food stand, matches its set's subjects
        // through its authored type family (`foodstand_burger` for
        // `foodstand_burger_df`) as well as its own definition.
        let resolve = |entity, set| {
            let (species, staff, guest, object) = sources.identities.get(entity).ok()?;
            let object_types = object
                .and_then(|object| definitions.find_object(object.0))
                .map_or(&[][..], |object| {
                    object.behavior_type_identifiers.as_slice()
                });
            declarations.find_behavior_set_location(
                set,
                behavior_subject_type_identifiers(
                    species,
                    staff,
                    guest,
                    definitions,
                    animal_types.borrow_for(entity),
                )
                .chain(object.map(|object| object.0))
                .chain(object_types.iter().copied()),
            )
        };
        let (subject_set, target_set) = (
            resolve(actor, action.subject_behavior_set),
            resolve(target, action.target_behavior_set),
        );
        let (
            Some((subject_document, subject_declaration)),
            Some((target_document, target_declaration)),
        ) = (subject_set.clone(), target_set.clone())
        else {
            debug!(
                ?actor,
                ?target,
                subject_set_found = subject_set.is_some(),
                target_set_found = target_set.is_some(),
                "synchronized behavior set has no declaration for its participant's types"
            );
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let origin = task.create_return_frame_after_action(task.action);
        let depth = task.stack.len();
        if !task.push_return_frame(origin.clone()) {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        claims_this_tick.insert(target);
        let target_execution_id = execution_sequence.next();
        commands
            .entity(target)
            .insert((
                SynchronizedBehaviorParticipant {
                    owner: actor,
                    execution_id: target_execution_id,
                },
                BehaviorTaskExecutionState {
                    execution_id: target_execution_id,
                    program: action.target_behavior_set,
                    target: Some(actor),
                    document: target_document,
                    declaration: target_declaration,
                    phase: BehaviorTaskExecutionPhase::Set,
                    action: 0,
                    repetitions: 0,
                    next_action_tick: clock.tick.saturating_add(1),
                    object: None,
                    interaction_slot: None,
                    stack: Default::default(),
                },
            ))
            .remove::<super::super::behavior_set_start_request_types::PendingBehaviorSet>();
        commands.entity(actor).insert(SynchronizedBehaviorOwner {
            execution_id: task.execution_id,
            target_execution_id,
            target,
            origin,
            depth,
            target_outcome: None,
        });
        task.program = action.subject_behavior_set;
        task.document = subject_document;
        task.declaration = subject_declaration;
        task.phase = BehaviorTaskExecutionPhase::Set;
        task.action = 0;
        task.repetitions = 0;
        task.next_action_tick = clock.tick.saturating_add(1);
        if action.reset_animation_phase {
            for (owner, mut controller) in &mut actors.controllers {
                if owner.gameplay_entity == actor || owner.gameplay_entity == target {
                    controller.elapsed_milliseconds = 0;
                }
            }
        }
    }
}
