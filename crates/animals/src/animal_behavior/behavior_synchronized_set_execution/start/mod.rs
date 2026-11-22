use super::super::{
    behavior_subject_type_resolution::behavior_subject_type_identifiers,
    behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
};
use super::{
    SynchronizedBehaviorOwner, SynchronizedBehaviorParticipant, SynchronizedBehaviorSources,
};
use bevy::prelude::*;
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_data::AssetId;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentations;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::animal_behavior::behavior_synchronized_set_types::{
    SynchronizedBehaviorOperation, SynchronizedBehaviorPartner, SynchronizedBehaviorSubject,
};
use openzt2_zoo_state::behavior_task_execution_types::running_actions::behavior_action_actors;
use openzt2_zoo_state::behavior_task_execution_types::{
    abandon_behavior_task_for_reconsideration, allocate_behavior_execution_id,
    running_actions::{
        BehaviorInstructionId, RunBehaviorSynchronizedSets, RunningBehaviorInstruction,
        SelectedBehaviorSet,
    },
    BehaviorTaskExecutionPhase, BehaviorTaskExecutionState, BehaviorTaskGeneration,
    PendingBehaviorTaskFailure, MAXIMUM_BEHAVIOR_TASK_RETURN_FRAME_COUNT,
};

pub(super) fn start_synchronized_behavior_sets(
    activation: In<Option<Entity>>,
    mut commands: Commands,
    clock: Single<&ZooClock>,
    sources: SynchronizedBehaviorSources,
    animal_types: super::super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    mut actors: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorSynchronizedSets,
        ),
        (
            Without<SynchronizedBehaviorOwner>,
            Without<SynchronizedBehaviorParticipant>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
) {
    let (Some(definitions), Some(declarations)) = (
        sources.definitions.get(&sources.definition_assets),
        sources
            .declarations
            .create_declaration_index_view(&sources.documents),
    ) else {
        return;
    };
    for (actor, task, instruction, running) in behavior_action_actors(&mut actors, activation.0) {
        if activation.0.is_none() && !instruction.can_update_after_activation(clock.tick) {
            continue;
        }
        if task.next_action_tick > clock.tick || !instruction.is_current(task) {
            continue;
        }
        let action = &running.0;
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
        let participants = task
            .target()
            .filter(|target| *target != actor)
            .and_then(|target| {
                Some(PreparedSynchronization {
                    target,
                    subject_set: action.subject_behavior_set,
                    subject: resolve(actor, action.subject_behavior_set)?,
                    partner_set: action.target_behavior_set,
                    partner: resolve(target, action.target_behavior_set)?,
                    reset_animation_phase: action.reset_animation_phase,
                })
            });
        commands.trigger(ClaimSynchronizedBehavior {
            actor,
            instruction: instruction.id(),
            tick: clock.tick,
            participants,
        });
    }
}

struct PreparedSynchronization {
    target: Entity,
    subject_set: AssetId,
    subject: (Handle<BehaviorDocumentAsset>, usize),
    partner_set: AssetId,
    partner: (Handle<BehaviorDocumentAsset>, usize),
    reset_animation_phase: bool,
}

#[derive(EntityEvent)]
pub(super) struct ClaimSynchronizedBehavior {
    #[event_target]
    actor: Entity,
    instruction: BehaviorInstructionId,
    tick: u64,
    participants: Option<PreparedSynchronization>,
}

pub(super) fn claim_synchronized_behavior(
    request: On<ClaimSynchronizedBehavior>,
    mut commands: Commands,
    actors: Query<
        (&BehaviorTaskExecutionState, &RunningBehaviorInstruction),
        (
            Without<SynchronizedBehaviorOwner>,
            Without<SynchronizedBehaviorParticipant>,
            Without<PendingBehaviorTaskFailure>,
        ),
    >,
    targets: Query<(
        Has<SynchronizedBehaviorOwner>,
        Has<SynchronizedBehaviorParticipant>,
        Has<BehaviorTaskExecutionState>,
    )>,
    mut execution_generations: Query<&mut BehaviorTaskGeneration>,
    presentations: Query<&AnimationPresentations>,
    mut players: Query<&mut AnimationPlayer>,
) {
    let actor = request.actor;
    let Ok((task, instruction)) = actors.get(actor) else {
        return;
    };
    if instruction.id() != request.instruction || !instruction.is_current(task) {
        return;
    }
    let Some(prepared) = request.participants.as_ref() else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    let target = prepared.target;
    let Ok((false, false, busy)) = targets.get(target) else {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    };
    let depth = task.depth;
    if depth == MAXIMUM_BEHAVIOR_TASK_RETURN_FRAME_COUNT {
        mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        return;
    }
    if busy {
        abandon_behavior_task_for_reconsideration(target, &mut commands);
    }
    let target_execution_id = allocate_behavior_execution_id(
        target,
        execution_generations.get_mut(target).ok(),
        &mut commands,
    );
    commands.spawn((
        SynchronizedBehaviorSubject(actor),
        SynchronizedBehaviorPartner(target),
        SynchronizedBehaviorOperation {
            execution_id: task.execution_id,
            target_execution_id,
            origin: task.create_return_frame_after_action(task.action),
            depth,
            target_outcome: None,
        },
    ));
    commands
        .entity(target)
        .insert(BehaviorTaskExecutionState {
            execution_id: target_execution_id,
            program: prepared.partner_set,
            target: Some(actor),
            document: prepared.partner.0.clone(),
            declaration: prepared.partner.1,
            phase: BehaviorTaskExecutionPhase::Set,
            action: 0,
            repetitions: 0,
            next_action_tick: request.tick.saturating_add(1),
            object: None,
            interaction_slot: None,
            depth: 0,
        })
        .remove::<super::super::behavior_set_start_request_types::PendingBehaviorSet>();
    instruction.enter_set(
        actor,
        SelectedBehaviorSet {
            program: prepared.subject_set,
            document: prepared.subject.0.clone(),
            declaration: prepared.subject.1,
            tick: request.tick,
            repeat: Some(task.repetitions),
            target: None,
        },
        &mut commands,
    );
    if prepared.reset_animation_phase {
        for subject in [actor, target] {
            for controller in
                AnimationPresentations::for_subject(subject, presentations.get(subject).ok())
            {
                if let Ok(mut player) = players.get_mut(controller) {
                    for (_, active) in player.playing_animations_mut() {
                        active.set_seek_time(0.0);
                    }
                }
            }
        }
    }
}
