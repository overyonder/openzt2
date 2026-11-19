//! Typed components for the instruction currently executing on a behavior actor.

use bevy::{
    ecs::{component::ComponentId, lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_data::{
    behavior::{action, action_record::BehaviorAction},
    AssetId,
};

use super::continuations::{
    suspend_behavior_frame, BehaviorContinuationFrames, BehaviorContinuations,
};
use super::MAXIMUM_BEHAVIOR_TASK_RETURN_FRAME_COUNT;
use super::{
    advance_behavior_task_to_next_action, BehaviorTaskExecutionPhase, BehaviorTaskExecutionState,
    BehaviorTaskReturnFrame,
};

#[derive(Component, PartialEq, Eq)]
pub struct BehaviorLocomotionIntent(pub action::wander::BehaviorGroundLocomotionSpeed);

#[derive(Component)]
pub struct RunBehaviorScript {
    pub context: AssetId,
    pub file: String,
    pub function: String,
    pub parameters: Vec<String>,
}

#[derive(Component)]
pub struct RunBehaviorTraversalDock {
    pub subject_node: String,
    pub target_path: String,
    pub clip: Option<String>,
}

#[derive(Component)]
pub struct RunBehaviorTraversalAnimation {
    pub subject_node: String,
    pub target_path: String,
    pub exit: bool,
}

#[derive(Component)]
pub struct RunBehaviorMorphEntity {
    pub recipient: openzt2_game_data::behavior::action_record::BehaviorMorphRecipient,
    pub definition: AssetId,
    pub name_it: bool,
    pub fade: bool,
    pub period: std::time::Duration,
}

#[derive(Component)]
pub struct RunBehaviorAnimationClip(pub action::animation::BehaviorAnimationClipAction);

#[derive(Component)]
pub struct RunBehaviorAttachObject(pub action::attach_object::BehaviorAttachObjectAction);

#[derive(Component)]
pub struct RunBehaviorDetachObject(pub action::attach_object::BehaviorDetachObjectAction);

#[derive(Component)]
pub struct RunBehaviorHide {
    pub minimum: std::time::Duration,
    pub maximum: std::time::Duration,
}

#[derive(Component)]
pub struct RunBehaviorSpawnObject {
    pub entity: AssetId,
    pub subject_node: String,
    pub animation: String,
    pub pose: action::spawn::BehaviorSpawnPosePolicy,
}

#[derive(Component)]
pub struct RunBehaviorSpawnChild {
    pub entity: AssetId,
    pub pose: action::spawn::BehaviorSpawnPosePolicy,
}

#[derive(Component)]
pub struct RunBehaviorSpawnObjectAtActor {
    pub entity: AssetId,
    pub pose: action::spawn::BehaviorSpawnPosePolicy,
}

#[derive(Component)]
pub struct RunBehaviorDock(pub action::docking::BehaviorDockAction);

#[derive(Component)]
pub struct RunBehaviorDockWater(pub action::docking::BehaviorWaterDockAction);

#[derive(Component)]
pub struct RunBehaviorMove(pub action::movement::BehaviorMoveAction);

#[derive(Component)]
pub struct RunBehaviorPursuit(pub action::movement::BehaviorPursuitAction);

#[derive(Component)]
pub struct RunBehaviorFaceTarget(pub action::face_target::BehaviorFaceTargetAction);

#[derive(Component)]
pub struct RunBehaviorExitTankGate;

#[derive(Component)]
pub struct RunBehaviorEvasion(pub action::wander::BehaviorEvasionAction);

#[derive(Component)]
pub struct RunBehaviorWaitInteractionQueue(pub action::queue::BehaviorQueueWaitAction);

#[derive(Component)]
pub struct RunBehaviorDockQueue(pub action::queue::BehaviorDockQueueAction);

#[derive(Component)]
pub struct RunBehaviorSynchronizedSets(
    pub action::synchronized_sets::BehaviorSynchronizedSetsAction,
);

#[derive(Component)]
pub struct RunBehaviorEnterInteractionContainer {
    pub container: Option<AssetId>,
    pub entrance_behavior_set: Option<AssetId>,
}

#[derive(Component)]
pub struct RunBehaviorFeedback(pub action::feedback::BehaviorFeedbackAction);

#[derive(Component)]
pub struct RunBehaviorViewAnimals(pub action::view_animals::BehaviorViewAnimalsAction);

#[derive(Component)]
pub struct BehaviorViewingProgress {
    pub instruction: BehaviorInstructionId,
    pub remaining: f32,
    pub maximum: f32,
}

impl BehaviorViewingProgress {
    pub fn apply_view_score(&mut self, score: f32) {
        self.remaining = (self.remaining + score).min(self.maximum);
    }
}

#[derive(Component)]
pub struct RunBehaviorViewEvent(pub action::view_event::BehaviorViewEventAction);

#[derive(Component)]
pub struct RunBehaviorWatchShow(pub action::watch_show::BehaviorWatchShowAction);

#[derive(Component)]
pub struct RunBehaviorFactModifications(pub Vec<action::modification::BehaviorFactModification>);

#[derive(Component)]
pub struct RunBehaviorLearnTrick {
    pub trick: AssetId,
    pub amount: u16,
    pub cap: u16,
}

#[derive(Component)]
pub struct RunBehaviorCureDisease;

#[derive(Component)]
pub struct RunBehaviorPlaySet(pub action::play_set::BehaviorPlaySetAction);

#[derive(Component)]
pub struct RunBehaviorTargetTest(pub action::target_test::BehaviorTargetTestAction);

#[derive(Component)]
pub struct RunBehaviorTermination(pub action::termination::BehaviorTerminationAction);

#[derive(Component)]
pub struct RunBehaviorEconomy {
    pub transaction: AssetId,
    pub cost_override: Option<f32>,
    pub tracker: Option<AssetId>,
    pub general: bool,
    pub keeper_refill: Option<BehaviorRefillContinuation>,
}

#[derive(Clone, Copy)]
pub struct BehaviorRefillContinuation {
    pub phase: BehaviorTaskExecutionPhase,
    pub action_after_refill: usize,
    pub amount_q16: i32,
}

/// A world-owned authored account tracker, attached to the active world.
#[derive(Component)]
pub struct BehaviorEconomyTracker {
    pub name: AssetId,
    pub transactions: Vec<
        openzt2_game_data::world_definitions::economy_transactions::EconomyTransactionDefinition,
    >,
}

#[derive(Component)]
pub struct RunningBehaviorEconomyTransaction {
    pub instruction: BehaviorInstructionId,
    pub facility: Entity,
    pub target: Option<Entity>,
    pub transaction: AssetId,
    pub cost_override: Option<f32>,
    pub payment: Option<Entity>,
    pub next_transaction: Option<AssetId>,
}

impl RunningBehaviorEconomyTransaction {
    pub fn owns(
        &self,
        task: &BehaviorTaskExecutionState,
        instruction: &RunningBehaviorInstruction,
    ) -> bool {
        self.instruction == instruction.id()
            && task.target == self.target
            && instruction.is_current(task)
    }
}

#[derive(Component)]
pub struct RunBehaviorRandomSet {
    pub weighted_sets: Vec<(AssetId, f32)>,
    pub minimum_plays: u32,
    pub maximum_plays: u32,
    pub looping: bool,
}

#[derive(Component)]
pub struct RunBehaviorRandomAnimation {
    pub weighted_clips: Vec<(String, f32)>,
    pub minimum_plays: u32,
    pub maximum_plays: u32,
    pub looping: bool,
}

#[derive(Component)]
pub struct RunBehaviorPlaceTarget(pub action::place_target::BehaviorPlaceTargetAction);

#[derive(Component)]
pub struct RunBehaviorMorphTarget(pub AssetId);

#[derive(Component)]
pub struct RunBehaviorDestroyTarget;

#[derive(Component)]
pub struct RunBehaviorLocomotionSwitchSet(
    pub action::locomotion_switch_set::BehaviorLocomotionSwitchSetAction,
);

#[derive(Component)]
pub struct RunBehaviorAnimationSwitchSet(
    pub action::locomotion_switch_set::BehaviorAnimationSwitchSetAction,
);

#[derive(Component)]
pub struct RunBehaviorAnimalShowEvent(pub action::show_event::BehaviorShowEventAction);

#[derive(Component)]
pub struct RunBehaviorHeadLook(pub action::head_look::BehaviorHeadLookAction);

#[derive(Component)]
pub struct RunBehaviorWander(pub action::wander::BehaviorWanderAction);

#[derive(Component)]
pub struct RunBehaviorSendSelfTokens(pub Vec<action::token::BehaviorSelfTokenDelivery>);

#[derive(Component)]
pub struct RunBehaviorSendSubjectTrigger(pub AssetId);

#[derive(Component)]
pub struct RunningBehaviorInstruction {
    id: BehaviorInstructionId,
    execution_id: u64,
    origin: BehaviorTaskReturnFrame,
    depth: usize,
    component: ComponentId,
    reservation: Option<AssetId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BehaviorInstructionId(u64);

#[derive(Resource, Default)]
struct BehaviorInstructionSequence(u64);

impl RunningBehaviorInstruction {
    #[must_use]
    pub const fn id(&self) -> BehaviorInstructionId {
        self.id
    }

    #[must_use]
    pub const fn reservation_tag(&self) -> Option<AssetId> {
        self.reservation
    }

    pub const fn execution_id(&self) -> u64 {
        self.execution_id
    }

    pub fn is_current(&self, task: &BehaviorTaskExecutionState) -> bool {
        self.execution_id == task.execution_id
            && self.depth == task.depth
            && task.is_at_exact_frame(&self.origin, self.depth)
    }

    pub fn complete(&self, actor: Entity, tick: u64, commands: &mut Commands) {
        complete_behavior_instruction(actor, self.id, tick, commands);
    }

    pub fn resume_at(&self, actor: Entity, tick: u64, commands: &mut Commands) {
        commands.trigger(BehaviorInstructionCompleted {
            actor,
            id: self.id,
            outcome: InstructionOutcome::ResumeAt(tick),
        });
    }

    pub fn repeat_at(&self, actor: Entity, tick: u64, repetitions: u32, commands: &mut Commands) {
        self.apply(
            actor,
            InstructionOutcome::RepeatAt { tick, repetitions },
            commands,
        );
    }

    pub fn enter_set(
        &self,
        actor: Entity,
        selection: SelectedBehaviorSet,
        commands: &mut Commands,
    ) {
        self.apply(actor, InstructionOutcome::EnterSet(selection), commands);
    }

    pub fn bind_interaction_slot(&self, actor: Entity, slot: usize, commands: &mut Commands) {
        self.apply(
            actor,
            InstructionOutcome::BindInteractionSlot(Some(slot)),
            commands,
        );
    }

    pub fn clear_interaction_slot(&self, actor: Entity, commands: &mut Commands) {
        self.apply(
            actor,
            InstructionOutcome::BindInteractionSlot(None),
            commands,
        );
    }

    pub fn complete_with_target(
        &self,
        actor: Entity,
        target: Option<Entity>,
        tick: u64,
        commands: &mut Commands,
    ) {
        self.apply(
            actor,
            InstructionOutcome::CompleteWithTarget { target, tick },
            commands,
        );
    }

    pub fn retarget(&self, actor: Entity, target: Option<Entity>, commands: &mut Commands) {
        self.apply(actor, InstructionOutcome::Retarget(target), commands);
    }

    fn apply(&self, actor: Entity, outcome: InstructionOutcome, commands: &mut Commands) {
        commands.trigger(BehaviorInstructionCompleted {
            actor,
            id: self.id,
            outcome,
        });
    }
}

pub fn complete_behavior_instruction(
    actor: Entity,
    id: BehaviorInstructionId,
    tick: u64,
    commands: &mut Commands,
) {
    commands.trigger(BehaviorInstructionCompleted {
        actor,
        id,
        outcome: InstructionOutcome::Complete(tick),
    });
}

pub fn complete_behavior_refill(
    actor: Entity,
    id: BehaviorInstructionId,
    tick: u64,
    after: BehaviorRefillContinuation,
    commands: &mut Commands,
) {
    commands.trigger(BehaviorInstructionCompleted {
        actor,
        id,
        outcome: InstructionOutcome::CompleteRefill { tick, after },
    });
}

pub struct SelectedBehaviorSet {
    pub program: AssetId,
    pub document: Handle<BehaviorDocumentAsset>,
    pub declaration: usize,
    pub tick: u64,
    /// The return to this instruction retains the repetition count.
    pub repeat: Option<u32>,
    /// Some(None) explicitly clears the target.
    pub target: Option<Option<Entity>>,
}

enum InstructionOutcome {
    Complete(u64),
    CompleteRefill {
        tick: u64,
        after: BehaviorRefillContinuation,
    },
    Retarget(Option<Entity>),
    ResumeAt(u64),
    RepeatAt {
        tick: u64,
        repetitions: u32,
    },
    EnterSet(SelectedBehaviorSet),
    BindInteractionSlot(Option<usize>),
    CompleteWithTarget {
        target: Option<Entity>,
        tick: u64,
    },
}

#[derive(EntityEvent)]
pub struct BehaviorInstructionCompleted {
    #[event_target]
    actor: Entity,
    id: BehaviorInstructionId,
    outcome: InstructionOutcome,
}

pub fn apply_behavior_instruction_completion(
    completion: On<BehaviorInstructionCompleted>,
    mut tasks: Query<(&mut BehaviorTaskExecutionState, &RunningBehaviorInstruction)>,
    mut commands: Commands,
) {
    let Ok((mut task, instruction)) = tasks.get_mut(completion.actor) else {
        return;
    };
    if completion.id == instruction.id && instruction.is_current(&task) {
        match &completion.outcome {
            InstructionOutcome::Retarget(target) => {
                task.target = *target;
                return;
            }
            InstructionOutcome::Complete(tick) => {
                advance_behavior_task_to_next_action(&mut task, *tick)
            }
            InstructionOutcome::CompleteRefill { tick, after } => {
                task.phase = after.phase;
                task.action = after.action_after_refill;
                task.repetitions = 0;
                task.next_action_tick = tick.saturating_add(1);
            }
            InstructionOutcome::CompleteWithTarget { target, tick } => {
                task.target = *target;
                advance_behavior_task_to_next_action(&mut task, *tick);
            }
            InstructionOutcome::ResumeAt(tick) => {
                task.next_action_tick = *tick;
                return;
            }
            InstructionOutcome::RepeatAt { tick, repetitions } => {
                task.next_action_tick = *tick;
                task.repetitions = *repetitions;
                return;
            }
            InstructionOutcome::BindInteractionSlot(slot) => {
                task.interaction_slot = *slot;
                return;
            }
            InstructionOutcome::EnterSet(set) => {
                let return_action = if set.repeat.is_some() {
                    task.action
                } else {
                    task.action.saturating_add(1)
                };
                let mut frame = task.create_return_frame_after_action(return_action);
                if let Some(repetitions) = set.repeat {
                    frame.repetitions = repetitions;
                }
                if task.depth >= MAXIMUM_BEHAVIOR_TASK_RETURN_FRAME_COUNT {
                    crate::animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation(completion.actor, &mut commands);
                    return;
                }
                suspend_behavior_frame(completion.actor, frame, &mut commands);
                task.depth += 1;
                task.program = set.program;
                task.document = set.document.clone();
                task.declaration = set.declaration;
                task.phase = BehaviorTaskExecutionPhase::Set;
                task.action = 0;
                task.repetitions = 0;
                task.next_action_tick = set.tick.saturating_add(1);
                if let Some(target) = set.target {
                    task.target = target;
                }
            }
        }
        let actor = completion.actor;
        commands.queue(move |world: &mut World| materialize_behavior_instruction(world, actor));
    }
}

#[derive(EntityEvent)]
pub struct ResumeSuspendedBehaviorInstruction {
    #[event_target]
    pub actor: Entity,
    pub execution_id: u64,
    pub origin: BehaviorTaskReturnFrame,
    pub depth: usize,
    pub instruction: BehaviorTaskReturnFrame,
    pub tick: u64,
    pub advance: bool,
}

pub fn resume_suspended_behavior_instruction(
    resume: On<ResumeSuspendedBehaviorInstruction>,
    mut tasks: Query<&mut BehaviorTaskExecutionState>,
    continuations: BehaviorContinuationFrames,
    mut commands: Commands,
) {
    let Ok(mut task) = tasks.get_mut(resume.actor) else {
        return;
    };
    if task.execution_id != resume.execution_id
        || !continuations.owns(resume.actor, &task, &resume.origin, resume.depth)
    {
        return;
    }
    continuations.truncate(resume.actor, resume.depth, &mut commands);
    task.restore_suspended_instruction(&resume.instruction, resume.depth, resume.tick);
    if resume.advance {
        advance_behavior_task_to_next_action(&mut task, resume.tick);
    }
    let actor = resume.actor;
    commands.queue(move |world: &mut World| materialize_behavior_instruction(world, actor));
}

pub fn refresh_running_behavior_instructions(
    tasks: Query<
        (
            Entity,
            &BehaviorTaskExecutionState,
            Option<&RunningBehaviorInstruction>,
        ),
        Or<(
            Changed<BehaviorTaskExecutionState>,
            Without<RunningBehaviorInstruction>,
        )>,
    >,
    mut commands: Commands,
) {
    for (actor, task, instruction) in &tasks {
        if instruction.is_none_or(|instruction| !instruction.is_current(task)) {
            commands.queue(move |world: &mut World| materialize_behavior_instruction(world, actor));
        }
    }
}

pub(super) fn initialize_running_behavior_instruction(
    mut world: DeferredWorld,
    context: HookContext,
) {
    let actor = context.entity;
    world
        .commands()
        .queue(move |world: &mut World| materialize_behavior_instruction(world, actor));
}

pub(super) fn discard_running_behavior_instruction(mut world: DeferredWorld, context: HookContext) {
    let discarded = world
        .get::<RunningBehaviorInstruction>(context.entity)
        .map(|instruction| (instruction.id, instruction.component));
    world.commands().entity(context.entity).queue_silenced(
        move |mut actor: bevy::ecs::world::EntityWorldMut<'_>| {
            actor.despawn_related::<crate::animal_behavior::behavior_target_placement_types::BehaviorPlacements>();
            actor.despawn_related::<BehaviorContinuations>();
            if let Some((id, component)) = discarded {
                if actor.get::<RunningBehaviorInstruction>().is_some_and(|instruction| instruction.id == id) {
                    actor.remove_by_id(component).remove::<(RunningBehaviorInstruction, BehaviorLocomotionIntent)>();
                }
            }
        },
    );
}

pub fn refresh_behavior_instructions_after_document_changes(
    mut events: MessageReader<AssetEvent<BehaviorDocumentAsset>>,
    tasks: Query<(Entity, &BehaviorTaskExecutionState)>,
    mut commands: Commands,
) {
    let changed = events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Modified { id } | AssetEvent::Removed { id } => Some(id),
            _ => None,
        })
        .collect::<std::collections::HashSet<_>>();
    if changed.is_empty() {
        return;
    }
    for (actor, task) in &tasks {
        if changed.contains(&task.document.id()) {
            commands.queue(move |world: &mut World| lower_behavior_instruction(world, actor, true));
        }
    }
}

fn materialize_behavior_instruction(world: &mut World, actor: Entity) {
    lower_behavior_instruction(world, actor, false);
}

fn lower_behavior_instruction(world: &mut World, actor: Entity, force: bool) {
    let Some(task) = world.get::<BehaviorTaskExecutionState>(actor) else {
        return;
    };
    let old_instruction = world.get::<RunningBehaviorInstruction>(actor);
    if !force && old_instruction.is_some_and(|instruction| instruction.is_current(task)) {
        return;
    }
    let old_component = old_instruction.map(|instruction| instruction.component);
    let document = world
        .get_resource::<Assets<BehaviorDocumentAsset>>()
        .and_then(|documents| documents.get(&task.document));
    let action = document
        .and_then(|document| {
            task.phase
                .authored_action(document, task.declaration, task.action)
        })
        .cloned();
    let keeper_refill =
        document.and_then(|document| resolve_refill_continuation(document, task, action.as_ref()?));
    let reservation = world
        .get_resource::<Assets<BehaviorDocumentAsset>>()
        .and_then(|documents| {
            let nested = world
                .get::<BehaviorContinuations>(actor)
                .into_iter()
                .flat_map(|links| bevy::ecs::relationship::RelationshipTarget::iter(links))
                .filter_map(|related| {
                    world.get::<super::continuations::SuspendedBehaviorFrame>(related)
                })
                .map(|frame| (&frame.0.document, frame.0.declaration, frame.0.phase))
                .collect::<Vec<_>>();
            std::iter::once((&task.document, task.declaration, task.phase))
                .chain(nested.into_iter().rev())
                .find_map(|(handle, declaration, phase)| {
                    if phase == BehaviorTaskExecutionPhase::Set {
                        return None;
                    }
                    documents
                        .get(handle)?
                        .behavior_task_at_index(declaration)?
                        .reservation_tag
                })
        });
    let instruction = (
        task.execution_id,
        task.create_return_frame_after_action(task.action),
        task.depth,
        reservation,
    );
    if let Some(component) = old_component {
        world
            .entity_mut(actor)
            .remove_by_id(component)
            .remove::<RunningBehaviorInstruction>();
    }
    let speed = match &action {
        Some(BehaviorAction::Wander(action)) => Some(action.locomotion_speed),
        Some(BehaviorAction::Pursuit(action)) => Some(action.locomotion_speed),
        Some(BehaviorAction::Evasion(action)) => Some(action.locomotion_speed),
        Some(BehaviorAction::Move(action)) => Some(action.locomotion_speed.unwrap_or_default()),
        _ => None,
    };
    let mut entity = world.entity_mut(actor);
    if let Some(speed) = speed {
        if let Some(mut intent) = entity.get_mut::<BehaviorLocomotionIntent>() {
            intent.set_if_neq(BehaviorLocomotionIntent(speed));
        } else {
            entity.insert(BehaviorLocomotionIntent(speed));
        }
    } else {
        entity.remove::<BehaviorLocomotionIntent>();
    }
    let Some(action) = action else {
        return;
    };
    match action {
        BehaviorAction::Script {
            context,
            file,
            function,
            parameters,
        } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorScript {
                context,
                file,
                function,
                parameters,
            },
        ),
        BehaviorAction::DockTraversal {
            subject_node,
            target_path,
            clip,
        } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorTraversalDock {
                subject_node,
                target_path,
                clip,
            },
        ),
        BehaviorAction::AnimateTraversal {
            subject_node,
            target_path,
            exit,
        } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorTraversalAnimation {
                subject_node,
                target_path,
                exit,
            },
        ),
        BehaviorAction::MorphEntity {
            recipient,
            definition,
            name_it,
            fade,
            period,
        } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorMorphEntity {
                recipient,
                definition,
                name_it,
                fade,
                period,
            },
        ),
        BehaviorAction::AnimationClip(action) => {
            insert_action(world, actor, instruction, RunBehaviorAnimationClip(action))
        }
        BehaviorAction::AttachObject(action) => {
            insert_action(world, actor, instruction, RunBehaviorAttachObject(action))
        }
        BehaviorAction::DetachObject(action) => {
            insert_action(world, actor, instruction, RunBehaviorDetachObject(action))
        }
        BehaviorAction::Hide { minimum, maximum } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorHide { minimum, maximum },
        ),
        BehaviorAction::SpawnObject {
            entity,
            subject_node,
            animation,
            pose,
        } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorSpawnObject {
                entity,
                subject_node,
                animation,
                pose,
            },
        ),
        BehaviorAction::SpawnChild { entity, pose } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorSpawnChild { entity, pose },
        ),
        BehaviorAction::SpawnObjectAtActor { entity, pose } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorSpawnObjectAtActor { entity, pose },
        ),
        BehaviorAction::Dock(action) => {
            insert_action(world, actor, instruction, RunBehaviorDock(action))
        }
        BehaviorAction::DockWater(action) => {
            insert_action(world, actor, instruction, RunBehaviorDockWater(action))
        }
        BehaviorAction::Move(action) => {
            insert_action(world, actor, instruction, RunBehaviorMove(action))
        }
        BehaviorAction::Pursuit(action) => {
            insert_action(world, actor, instruction, RunBehaviorPursuit(action))
        }
        BehaviorAction::FaceTarget(action) => {
            insert_action(world, actor, instruction, RunBehaviorFaceTarget(action))
        }
        BehaviorAction::ExitTankGate => {
            insert_action(world, actor, instruction, RunBehaviorExitTankGate)
        }
        BehaviorAction::Evasion(action) => {
            insert_action(world, actor, instruction, RunBehaviorEvasion(action))
        }
        BehaviorAction::WaitInteractionQueue(action) => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorWaitInteractionQueue(action),
        ),
        BehaviorAction::DockQueue(action) => {
            insert_action(world, actor, instruction, RunBehaviorDockQueue(action))
        }
        BehaviorAction::SynchronizedSets(action) => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorSynchronizedSets(action),
        ),
        BehaviorAction::EnterInteractionContainer {
            container,
            entrance_behavior_set,
        } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorEnterInteractionContainer {
                container,
                entrance_behavior_set,
            },
        ),
        BehaviorAction::Feedback(action) => {
            insert_action(world, actor, instruction, RunBehaviorFeedback(action))
        }
        BehaviorAction::ViewAnimals(action) => {
            insert_action(world, actor, instruction, RunBehaviorViewAnimals(action))
        }
        BehaviorAction::ViewEvent(action) => {
            insert_action(world, actor, instruction, RunBehaviorViewEvent(action))
        }
        BehaviorAction::WatchShow(action) => {
            insert_action(world, actor, instruction, RunBehaviorWatchShow(action))
        }
        BehaviorAction::FactModifications(action) => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorFactModifications(action),
        ),
        BehaviorAction::LearnTrick { trick, amount, cap } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorLearnTrick { trick, amount, cap },
        ),
        BehaviorAction::CureDisease => {
            insert_action(world, actor, instruction, RunBehaviorCureDisease)
        }
        BehaviorAction::PlaySet(action) => {
            insert_action(world, actor, instruction, RunBehaviorPlaySet(action))
        }
        BehaviorAction::TargetTest(action) => {
            insert_action(world, actor, instruction, RunBehaviorTargetTest(action))
        }
        BehaviorAction::Termination(action) => {
            insert_action(world, actor, instruction, RunBehaviorTermination(action))
        }
        BehaviorAction::Economy {
            transaction,
            cost_override,
            tracker,
            general,
        } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorEconomy {
                transaction,
                cost_override,
                tracker,
                general,
                keeper_refill,
            },
        ),
        BehaviorAction::RandomSet {
            weighted_sets,
            minimum_plays,
            maximum_plays,
            looping,
        } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorRandomSet {
                weighted_sets,
                minimum_plays,
                maximum_plays,
                looping,
            },
        ),
        BehaviorAction::RandomAnimation {
            weighted_clips,
            minimum_plays,
            maximum_plays,
            looping,
        } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorRandomAnimation {
                weighted_clips,
                minimum_plays,
                maximum_plays,
                looping,
            },
        ),
        BehaviorAction::PlaceTarget(action) => {
            insert_action(world, actor, instruction, RunBehaviorPlaceTarget(action))
        }
        BehaviorAction::MorphTarget(action) => {
            insert_action(world, actor, instruction, RunBehaviorMorphTarget(action))
        }
        BehaviorAction::DestroyTarget => {
            insert_action(world, actor, instruction, RunBehaviorDestroyTarget)
        }
        BehaviorAction::LocomotionSwitchSet(action) => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorLocomotionSwitchSet(action),
        ),
        BehaviorAction::AnimationSwitchSet(action) => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorAnimationSwitchSet(action),
        ),
        BehaviorAction::AnimalShowEvent(action) => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorAnimalShowEvent(action),
        ),
        BehaviorAction::HeadLook(action) => {
            insert_action(world, actor, instruction, RunBehaviorHeadLook(action))
        }
        BehaviorAction::Wander(action) => {
            insert_action(world, actor, instruction, RunBehaviorWander(action))
        }
        BehaviorAction::SendSelfTokens(action) => {
            insert_action(world, actor, instruction, RunBehaviorSendSelfTokens(action))
        }
        BehaviorAction::SendSubjectTrigger(action) => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorSendSubjectTrigger(action),
        ),
    }
}

fn resolve_refill_continuation(
    document: &BehaviorDocumentAsset,
    task: &BehaviorTaskExecutionState,
    current_action: &BehaviorAction,
) -> Option<BehaviorRefillContinuation> {
    use openzt2_game_data::behavior::{
        action::{
            entity_role::BehaviorEntityRole,
            modification::{BehaviorFact, BehaviorFactModificationOperation},
        },
        scalar::BehaviorScalarQ16,
    };
    if task.phase != BehaviorTaskExecutionPhase::Execution || task.depth != 0 {
        return None;
    }
    let BehaviorAction::Economy {
        transaction,
        general: false,
        tracker: None,
        ..
    } = current_action
    else {
        return None;
    };
    if *transaction != AssetId::from_key("refill") {
        return None;
    }
    let declaration = document.behavior_task_at_index(task.declaration)?;
    let next_index = task.action.saturating_add(1);
    let (phase, index, next) =
        if let Some(next) = declaration.execution.lowered_actions().get(next_index) {
            (BehaviorTaskExecutionPhase::Execution, next_index, next)
        } else {
            if declaration.execution.has_unsupported_suffix() {
                return None;
            }
            (
                BehaviorTaskExecutionPhase::Completion,
                0,
                declaration.completion.lowered_actions().first()?,
            )
        };
    let BehaviorAction::FactModifications(modifications) = next else {
        return None;
    };
    let [modification] = modifications.as_slice() else {
        return None;
    };
    let BehaviorScalarQ16::FixedQ16(amount_q16) = modification.modification_value else {
        return None;
    };
    (amount_q16 > 0
        && modification.affected_entity_role == BehaviorEntityRole::Target
        && modification.modified_fact == BehaviorFact::FoodLevel
        && modification.modification_operation == BehaviorFactModificationOperation::Add)
        .then_some(BehaviorRefillContinuation {
            phase,
            action_after_refill: index.saturating_add(1),
            amount_q16,
        })
}

fn insert_action<A: Component>(
    world: &mut World,
    actor: Entity,
    (execution_id, origin, depth, reservation): (
        u64,
        BehaviorTaskReturnFrame,
        usize,
        Option<AssetId>,
    ),
    action: A,
) {
    let component = world.register_component::<A>();
    let mut sequence = world.get_resource_or_insert_with(BehaviorInstructionSequence::default);
    sequence.0 = sequence
        .0
        .checked_add(1)
        .expect("behavior instruction identity exhausted");
    let id = BehaviorInstructionId(sequence.0);
    world.entity_mut(actor).insert((
        RunningBehaviorInstruction {
            id,
            execution_id,
            origin,
            depth,
            component,
            reservation,
        },
        action,
    ));
}
