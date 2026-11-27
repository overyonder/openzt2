//! Typed components for the instruction currently executing on a behavior actor.

use bevy::{
    ecs::{
        component::ComponentId,
        event::{EntityComponentsTrigger, EventPattern},
        lifecycle::HookContext,
        query::{IterQueryData, QueryFilter, QueryItem},
        world::DeferredWorld,
    },
    prelude::*,
};
use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_data::{
    behavior::{action, action_record::BehaviorAction},
    AssetId,
};

use super::continuations::{BehaviorContinuationFrames, BehaviorContinuations};
use super::MAXIMUM_BEHAVIOR_TASK_RETURN_FRAME_COUNT;
use super::{
    advance_behavior_task_to_next_action, BehaviorTaskExecutionPhase, BehaviorTaskExecutionState,
    BehaviorTaskGeneration, BehaviorTaskReturnFrame, PendingBehaviorTaskFailure,
};
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

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

#[derive(Clone, Copy)]
pub enum BehaviorAnimationEffect {
    Detach {
        entity: AssetId,
        rule: AssetId,
    },
    Kill {
        termination: action::termination::BehaviorTerminationAction,
        radius: Option<(f32, AssetId)>,
        destroy: bool,
    },
    Whap {
        angle_degrees: Option<f32>,
        random_offset_degrees: Option<f32>,
    },
}

#[derive(Component)]
pub struct TimedBehaviorAnimationEffect(pub BehaviorAnimationEffect);

#[derive(Component)]
pub struct RunBehaviorAnimationEffect(pub BehaviorAnimationEffect);

#[derive(Component)]
pub struct BehaviorAnimationTextKeys(
    pub openzt2_game_data::behavior::action_record::BehaviorAnimationTextKeyAliases,
);

#[derive(Component)]
pub struct BehaviorPlaySetInterrupt(pub AssetId);

#[derive(Component)]
pub struct SuspendedBehaviorSetInterrupt {
    pub trigger: AssetId,
    pub target: Option<Entity>,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BehaviorAnimationKeyDelivery;

#[derive(Message, Clone)]
pub struct DispatchBehaviorAnimationTextKeys {
    pub controller: Entity,
    pub playback_generation: u64,
    pub request_id: usize,
    pub joint: Entity,
    pub milliseconds: u32,
    pub commands:
        Vec<openzt2_game_data::animation::animation_text_key::AuthoredAnimationTextCommand>,
}

#[derive(Component)]
pub struct RunBehaviorAnimationClip(pub action::animation::BehaviorAnimationClipAction);

#[derive(Component)]
pub struct RunBehaviorAnimationSwitch(pub action::animation::BehaviorAnimationSwitchAction);

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
    pub fade_in: std::time::Duration,
    pub within_habitat: bool,
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

/// Tour boarding stations this guest has visited.
#[derive(Component, Default)]
pub struct GuestVisitedTourStations(pub bevy::ecs::entity::EntityHashSet);

/// Source tour need and the current zoo's tour limit for departure.
#[derive(Component, Clone, Copy, PartialEq)]
pub struct GuestTourDepartureGate {
    pub need: f32,
    pub threshold: f32,
    pub policy:
        openzt2_game_data::world_definitions::guest_simulation_definitions::GuestTourNeedPolicy,
    pub triggered: bool,
}

impl GuestTourDepartureGate {
    pub fn adjust(&mut self, amount: f32) {
        self.need = (self.need + amount).min(100.0);
        if self.need >= self.policy.trigger {
            self.triggered = true;
        } else if self.need <= self.policy.cessation {
            self.triggered = false;
        }
    }

    pub fn source_need(&self) -> (i32, bool) {
        ((self.need * 65_536.0).round() as i32, self.triggered)
    }
}

#[derive(Component)]
pub struct BehaviorViewingProgress {
    pub instruction: BehaviorInstructionId,
    pub execution: u64,
    pub at: BehaviorTaskReturnFrame,
    pub depth: usize,
    pub remaining: f32,
    pub maximum: f32,
    pub emote_score: Option<f32>,
}

impl BehaviorViewingProgress {
    pub fn apply_view_score(&mut self, score: f32) {
        self.remaining = (self.remaining + score).min(self.maximum);
        if self
            .emote_score
            .is_none_or(|previous| previous.abs() < score.abs())
        {
            self.emote_score = Some(score);
        }
    }
}

#[derive(Component)]
pub struct RunBehaviorViewEvent(pub action::view_event::BehaviorViewEventAction);

/// A guest thought delivered by a view event. The guest owns it as a child.
#[derive(Component)]
pub struct AuthoredViewThought {
    pub localization_key: openzt2_game_data::AssetId,
    pub priority: i16,
    pub globally_visible: bool,
    pub source: Entity,
    pub arguments: [Option<Entity>; 5],
}

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
pub struct RunBehaviorDockNow(pub action::specialized::BehaviorDockNowAction);

#[derive(Component)]
pub struct RunBehaviorDockSpline(pub action::specialized::BehaviorDockSplineAction);

#[derive(Component)]
pub struct RunBehaviorFall(pub action::specialized::BehaviorFallAction);

#[derive(Component)]
pub struct RunBehaviorEscapeObstacle(pub action::specialized::BehaviorEscapeObstacleAction);

/// Escape steering considers the strongest blocking obstacles in range.
#[derive(Component, Clone, Copy, PartialEq)]
pub struct BehaviorObstaclePriority(pub u32);

#[derive(Component)]
pub struct RunBehaviorExitInteractionContainer(pub Option<AssetId>);

#[derive(Component)]
pub struct RunBehaviorFindArtifact;

#[derive(Component)]
pub struct RunBehaviorDigArtifact;

#[derive(Component)]
pub struct RunBehaviorTour;

#[derive(Component, Clone, Copy)]
pub struct AuthoredTourRide {
    pub source: Entity,
    pub destination: Entity,
}

#[derive(Component)]
pub struct RunBehaviorTeleportToLocation(pub action::specialized::BehaviorTeleportToLocationAction);

#[derive(Component)]
pub struct RunBehaviorKickOffRider;

#[derive(Component)]
pub struct RunBehaviorAnimateForViewFromTransport(pub String);

#[derive(Component)]
pub struct RunBehaviorFollowEntity(pub action::specialized::BehaviorFollowEntityAction);

#[derive(Component)]
pub struct RunBehaviorSplashGuest(pub action::specialized::BehaviorSplashGuestAction);

#[derive(Component)]
pub struct SplashAnimation {
    pub step: BehaviorInstructionId,
    pub request: usize,
    pub emitted: bool,
}

#[derive(Component, Clone, Copy)]
pub struct RestoredSplashAnimation {
    pub seek_seconds: f32,
    pub emitted: bool,
}

#[derive(Component)]
pub struct SplashAnimationSeek {
    pub request: usize,
    pub seconds: f32,
}

#[derive(Component)]
pub struct RunBehaviorUsePortal(pub action::specialized::BehaviorUsePortalAction);

#[derive(Component)]
pub struct RunBehaviorDockTankWall(pub action::specialized::BehaviorDockTankWallAction);

#[derive(Component)]
pub struct RunBehaviorStaffTrainAnimal(pub action::specialized::BehaviorStaffTrainAnimalAction);

/// The completed evaluation of a player gesture sequence.
#[derive(EntityEvent)]
pub struct CompletedPlayerTraining {
    pub entity: Entity,
    pub perfect: bool,
}

/// Authored training choices enabled for a particular trainer assignment.
#[derive(Component, Clone, PartialEq, Eq)]
pub struct TrainerEnabledTricks(pub Vec<AssetId>);

#[derive(Component)]
pub struct RunBehaviorAddSubjectAsDiseaseTarget(
    pub action::specialized::BehaviorAddSubjectAsDiseaseTargetAction,
);

#[derive(Component)]
pub struct RunBehaviorInvalidAuthoredAction(pub String);

#[derive(Component)]
pub struct RunBehaviorPlaceTarget(pub action::place_target::BehaviorPlaceTargetAction);

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
    activated_at: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BehaviorInstructionId(u64);

#[derive(Component)]
pub struct FollowingEntity {
    pub instruction: BehaviorInstructionId,
    pub target: Entity,
    pub until_tick: u64,
    pub request_id: u64,
}

#[derive(Component)]
pub struct SplineDockMotion {
    pub instruction: BehaviorInstructionId,
    pub target: Entity,
    pub phase: SplineDockPhase,
    pub owns_disabled_collider: bool,
}

#[derive(Component, Debug)]
pub struct SplineDockRetry(pub Timer);

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub enum SplineDockPhase {
    Approach(u64),
    Ready,
    Curve { start: Transform, advanced: f32 },
}

impl SplineDockPhase {
    pub const fn request(self) -> Option<u64> {
        match self {
            Self::Approach(id) => Some(id),
            Self::Ready | Self::Curve { .. } => None,
        }
    }
}

#[derive(Component)]
struct BehaviorInstructionGeneration(u64);

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

    pub fn can_update_after_activation(&self, tick: u64) -> bool {
        self.activated_at
            .is_some_and(|activated_at| activated_at < tick)
    }

    pub fn is_current(&self, task: &BehaviorTaskExecutionState) -> bool {
        self.execution_id == task.execution_id
            && self.depth == task.depth
            && task.is_at_exact_frame(&self.origin, self.depth)
    }

    pub fn complete(&self, actor: Entity, tick: u64, commands: &mut Commands) {
        complete_behavior_instruction(actor, self.id, tick, commands);
    }

    pub fn enter_set(
        &self,
        actor: Entity,
        selection: SelectedBehaviorSet,
        commands: &mut Commands,
    ) {
        self.apply(actor, InstructionOutcome::EnterSet(selection), commands);
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
    EnterSet(SelectedBehaviorSet),
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BehaviorStepFixedSet {
    Think,
    Act,
}

/// Applies the guest survey action before departures reach economy settlement.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GuestDepartureSurveyApplication;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GuestDepartureFameApplication;

#[derive(Component)]
pub struct BehaviorInstructionAwaitingActivation;

#[derive(EntityEvent)]
#[entity_event(trigger = EntityComponentsTrigger<'static>)]
pub struct BehaviorInstructionActivated {
    pub entity: Entity,
    pub instruction: BehaviorInstructionId,
}

pub struct WhenBehaviorAction<A: Bundle>(std::marker::PhantomData<A>);

impl<A: Bundle> EventPattern for WhenBehaviorAction<A> {
    type Event = BehaviorInstructionActivated;
    type Components = A;
}

pub fn activated_behavior_actor<A: Bundle>(
    activation: On<WhenBehaviorAction<A>>,
) -> Option<Entity> {
    Some(activation.entity)
}

pub const fn update_waiting_behavior_actors() -> Option<Entity> {
    None
}

pub fn behavior_action_actors<'q, 'w: 'q, 's: 'q, D: IterQueryData + 'q, F: QueryFilter + 'q>(
    query: &'q mut Query<'w, 's, D, F>,
    actor: Option<Entity>,
) -> impl Iterator<Item = QueryItem<'q, 's, D>> + 'q {
    let (mut all, mut one) = match actor {
        Some(actor) => (None, query.get_mut(actor).ok()),
        None => (Some(query.iter_mut()), None),
    };
    std::iter::from_fn(move || match &mut all {
        Some(all) => all.next(),
        None => one.take(),
    })
}

#[derive(EntityEvent)]
pub struct BehaviorProgramBoundary {
    pub entity: Entity,
    pub tick: u64,
}

pub fn activate_due_behavior_instructions(
    clocks: Query<&ZooClock>,
    actors: Query<
        (Entity, &WorldMember, &BehaviorTaskExecutionState),
        With<BehaviorInstructionAwaitingActivation>,
    >,
    mut commands: Commands,
) {
    for (actor, member, task) in &actors {
        let Ok(clock) = clocks.get(member.root) else {
            continue;
        };
        let tick = clock.tick;
        if task.next_action_tick <= tick {
            commands
                .queue(move |world: &mut World| activate_behavior_instruction(world, actor, tick));
        }
    }
}

#[track_caller]
pub fn mark_behavior_task_for_failure_and_stop_navigation(actor: Entity, commands: &mut Commands) {
    use crate::animal_behavior::behavior_pursuit_types::{
        BehaviorPursuitPoll, PendingBehaviorPursuit,
    };
    use crate::locomotion::locomotion_types::{Destination, Docking};

    debug!(?actor, site = %std::panic::Location::caller(), "behavior task marked for failure");
    commands
        .entity(actor)
        .try_remove::<(
            Destination,
            Docking,
            super::PendingBehaviorDockingCompletion,
            super::PendingBehaviorMoveCompletion,
            super::PendingBehaviorAnimationClipCompletion,
            PendingBehaviorPursuit,
            BehaviorPursuitPoll,
        )>()
        .try_insert(PendingBehaviorTaskFailure);
}

pub fn activate_behavior_failure(
    failure: On<Add<PendingBehaviorTaskFailure>>,
    clocks: Query<&ZooClock>,
    tasks: Query<(&BehaviorTaskExecutionState, Option<&WorldMember>)>,
    mut commands: Commands,
) {
    let Ok((task, member)) = tasks.get(failure.entity) else {
        return;
    };
    commands.trigger(BehaviorProgramBoundary {
        entity: failure.entity,
        tick: member
            .and_then(|member| clocks.get(member.root).ok())
            .map_or(task.next_action_tick, |clock| clock.tick),
    });
}

pub fn continue_behavior_program(actor: Entity, tick: u64, commands: &mut Commands) {
    commands.queue(move |world: &mut World| {
        materialize_behavior_instruction(world, actor);
        activate_behavior_instruction(world, actor, tick);
    });
}

fn activate_behavior_instruction(world: &mut World, actor: Entity, tick: u64) {
    use crate::animal_health::types::{Dead, Frozen, Tranquilized};
    use openzt2_world_state::world_spawn::world_entity_crating::{AnimalCrate, ScriptedCrating};

    let Some(task) = world.get::<BehaviorTaskExecutionState>(actor) else {
        return;
    };
    if task.next_action_tick > tick
        || world
            .get::<BehaviorInstructionAwaitingActivation>(actor)
            .is_none()
        || ((world.get::<Frozen>(actor).is_some() || world.get::<Tranquilized>(actor).is_some())
            && world.get::<RunBehaviorTermination>(actor).is_none())
        || world.get::<AnimalCrate>(actor).is_some()
        || world.get::<ScriptedCrating>(actor).is_some()
        || world.get::<Dead>(actor).is_some()
    {
        return;
    }
    let instruction = world
        .get::<RunningBehaviorInstruction>(actor)
        .filter(|instruction| instruction.is_current(task))
        .map(|instruction| (instruction.id, instruction.component));
    if instruction.is_none()
        && world
            .get_resource::<Assets<BehaviorDocumentAsset>>()
            .is_none_or(|documents| !documents.contains(&task.document))
    {
        return;
    }
    world
        .entity_mut(actor)
        .remove::<BehaviorInstructionAwaitingActivation>();
    if world.get::<PendingBehaviorTaskFailure>(actor).is_none() {
        if let Some((instruction, component)) = instruction {
            if let Some(mut active) = world.get_mut::<RunningBehaviorInstruction>(actor) {
                active.activated_at = Some(tick);
            }
            world.trigger_with(
                BehaviorInstructionActivated {
                    entity: actor,
                    instruction,
                },
                EntityComponentsTrigger {
                    components: std::slice::from_ref(&component),
                    ..Default::default()
                },
            );
            return;
        }
    }
    world.trigger(BehaviorProgramBoundary {
        entity: actor,
        tick,
    });
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
    mut tasks: Query<
        (
            &mut BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            Option<&BehaviorPlaySetInterrupt>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    mut commands: Commands,
) {
    let Ok((mut task, instruction, interrupt)) = tasks.get_mut(completion.actor) else {
        return;
    };
    if completion.id == instruction.id && instruction.is_current(&task) {
        match &completion.outcome {
            InstructionOutcome::Complete(tick) => {
                advance_behavior_task_to_next_action(&mut task, *tick)
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
                    mark_behavior_task_for_failure_and_stop_navigation(
                        completion.actor,
                        &mut commands,
                    );
                    return;
                }
                let mut suspended = commands.spawn((
                    super::continuations::BehaviorContinuationOf(completion.actor),
                    super::continuations::SuspendedBehaviorFrame(frame),
                ));
                if let Some(interrupt) = interrupt {
                    suspended.insert(SuspendedBehaviorSetInterrupt {
                        trigger: interrupt.0,
                        target: task.target,
                    });
                }
                if set.target.is_some() {
                    suspended.insert(super::continuations::SuspendedBehaviorTarget(task.target));
                }
                task.depth += 1;
                task.program = set.program;
                task.document = set.document.clone();
                task.declaration = set.declaration;
                task.phase = BehaviorTaskExecutionPhase::Set;
                task.action = 0;
                task.repetitions = 0;
                task.next_action_tick = set.tick;
                if let Some(target) = set.target {
                    task.target = target;
                }
            }
        }
        continue_behavior_program(completion.actor, task.next_action_tick, &mut commands);
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
    pub outcome: BehaviorResumeOutcome,
}

#[derive(Clone, Copy)]
pub enum BehaviorResumeOutcome {
    Retry,
    Advance,
    Fail,
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
    let target = (task.depth > resume.depth)
        .then(|| continuations.suspended_target(resume.actor, resume.depth))
        .flatten();
    continuations.truncate(resume.actor, resume.depth, &mut commands);
    task.restore_suspended_instruction(&resume.instruction, resume.depth, resume.tick);
    if let Some(target) = target {
        task.target = target;
    }
    match resume.outcome {
        BehaviorResumeOutcome::Retry => {}
        BehaviorResumeOutcome::Advance => {
            advance_behavior_task_to_next_action(&mut task, resume.tick);
        }
        BehaviorResumeOutcome::Fail => {
            mark_behavior_task_for_failure_and_stop_navigation(resume.actor, &mut commands);
            return;
        }
    }
    continue_behavior_program(resume.actor, resume.tick, &mut commands);
}

pub(super) fn initialize_running_behavior_instruction(
    mut world: DeferredWorld,
    context: HookContext,
) {
    let actor = context.entity;
    let execution = world
        .get::<BehaviorTaskExecutionState>(actor)
        .map(|task| task.execution_id);
    if let Some(execution) = execution {
        if let Some(mut generation) = world.get_mut::<BehaviorTaskGeneration>(actor) {
            generation.reserve_restored(execution);
        }
    }
    world
        .commands()
        .queue(move |world: &mut World| materialize_behavior_instruction(world, actor));
}

#[derive(EntityEvent)]
pub struct BehaviorTaskReplaced {
    #[event_target]
    pub actor: Entity,
    pub previous_execution: u64,
    pub previous_object: Option<Entity>,
    pub execution: u64,
}

pub fn reset_replaced_behavior_task(replaced: On<BehaviorTaskReplaced>, mut commands: Commands) {
    let actor = replaced.actor;
    let execution = replaced.execution;
    commands.queue(move |world: &mut World| {
        let Ok(mut entity) = world.get_entity_mut(actor) else {
            return;
        };
        if entity
            .get::<BehaviorTaskExecutionState>()
            .is_none_or(|task| task.execution_id != execution)
        {
            return;
        }
        entity.despawn_related::<crate::animal_behavior::behavior_target_placement_types::BehaviorPlacements>();
        entity.despawn_related::<BehaviorContinuations>();
        if let Some(component) = entity.get::<RunningBehaviorInstruction>().map(|step| step.component) {
            entity.remove_by_id(component).remove::<(
                RunningBehaviorInstruction,
                BehaviorLocomotionIntent,
                TimedBehaviorAnimationEffect,
                BehaviorAnimationTextKeys,
                BehaviorPlaySetInterrupt,
            )>();
        }
        materialize_behavior_instruction(world, actor);
    });
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
                    actor.remove_by_id(component).remove::<(RunningBehaviorInstruction, BehaviorLocomotionIntent, TimedBehaviorAnimationEffect, BehaviorAnimationTextKeys, BehaviorPlaySetInterrupt)>();
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
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(id)
            }
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
        world.entity_mut(actor).remove_by_id(component).remove::<(
            RunningBehaviorInstruction,
            TimedBehaviorAnimationEffect,
            BehaviorAnimationTextKeys,
            BehaviorPlaySetInterrupt,
        )>();
    }
    let speed = match &action {
        Some(BehaviorAction::Wander(action)) => Some(action.locomotion_speed),
        Some(BehaviorAction::DockSpline(action)) => Some(action.speed),
        Some(BehaviorAction::EscapeObstacle(action)) => Some(action.speed),
        Some(BehaviorAction::ExitInteractionContainer { .. }) => {
            Some(action::wander::BehaviorGroundLocomotionSpeed::Slow)
        }
        Some(BehaviorAction::FollowEntity(action)) => Some(action.speed),
        Some(BehaviorAction::UsePortal(action)) => Some(action.speed),
        Some(BehaviorAction::DockTankWall(action)) => Some(action.speed),
        Some(BehaviorAction::Pursuit(action)) => Some(action.locomotion_speed),
        Some(BehaviorAction::Evasion(action)) => Some(action.locomotion_speed),
        Some(BehaviorAction::Move(action)) => Some(action.locomotion_speed.unwrap_or_default()),
        _ => None,
    };
    let mut entity = world.entity_mut(actor);
    entity.insert_if_new(BehaviorInstructionAwaitingActivation);
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
        BehaviorAction::AnimationWithTextKeys { animation, keys } => {
            world
                .entity_mut(actor)
                .insert(BehaviorAnimationTextKeys(keys));
            insert_action(
                world,
                actor,
                instruction,
                RunBehaviorAnimationClip(animation),
            );
        }
        BehaviorAction::AttachWithTextKeys { attachment, keys } => {
            world
                .entity_mut(actor)
                .insert(BehaviorAnimationTextKeys(keys));
            insert_action(
                world,
                actor,
                instruction,
                RunBehaviorAttachObject(attachment),
            );
        }
        BehaviorAction::DetachWithAnimation {
            detach,
            animation,
            keys,
        } => {
            world.entity_mut(actor).insert((
                BehaviorAnimationTextKeys(keys),
                TimedBehaviorAnimationEffect(BehaviorAnimationEffect::Detach {
                    entity: detach.entity,
                    rule: detach.detach_rule,
                }),
            ));
            insert_action(
                world,
                actor,
                instruction,
                RunBehaviorAnimationClip(animation),
            );
        }
        BehaviorAction::AnimationKill {
            animation,
            termination,
            radius,
            destroy,
        } => {
            let effect = BehaviorAnimationEffect::Kill {
                termination,
                radius,
                destroy,
            };
            if let Some(animation) = animation {
                world
                    .entity_mut(actor)
                    .insert(TimedBehaviorAnimationEffect(effect));
                insert_action(
                    world,
                    actor,
                    instruction,
                    RunBehaviorAnimationClip(animation),
                );
            } else {
                insert_action(
                    world,
                    actor,
                    instruction,
                    RunBehaviorAnimationEffect(effect),
                );
            }
        }
        BehaviorAction::Whap {
            animation,
            angle_degrees,
            random_offset_degrees,
        } => {
            let effect = BehaviorAnimationEffect::Whap {
                angle_degrees,
                random_offset_degrees,
            };
            if let Some(animation) = animation {
                world
                    .entity_mut(actor)
                    .insert(TimedBehaviorAnimationEffect(effect));
                insert_action(
                    world,
                    actor,
                    instruction,
                    RunBehaviorAnimationClip(animation),
                );
            } else {
                insert_action(
                    world,
                    actor,
                    instruction,
                    RunBehaviorAnimationEffect(effect),
                );
            }
        }
        BehaviorAction::PlaySetWithInterrupt { set, interrupt } => {
            world
                .entity_mut(actor)
                .insert(BehaviorPlaySetInterrupt(interrupt));
            insert_action(world, actor, instruction, RunBehaviorPlaySet(set));
        }
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
        BehaviorAction::AnimationSwitch(action) => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorAnimationSwitch(action),
        ),
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
            RunBehaviorSpawnChild {
                entity,
                pose,
                fade_in: std::time::Duration::ZERO,
                within_habitat: false,
            },
        ),
        BehaviorAction::SpawnChildWithPlacement {
            entity,
            pose,
            fade_in,
            within_habitat,
        } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorSpawnChild {
                entity,
                pose,
                fade_in,
                within_habitat,
            },
        ),
        BehaviorAction::SpawnObjectAtActor { entity, pose } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorSpawnObjectAtActor { entity, pose },
        ),
        BehaviorAction::DockNow(action) => {
            insert_action(world, actor, instruction, RunBehaviorDockNow(action))
        }
        BehaviorAction::DockSpline(action) => {
            insert_action(world, actor, instruction, RunBehaviorDockSpline(action))
        }
        BehaviorAction::Fall(action) => {
            insert_action(world, actor, instruction, RunBehaviorFall(action))
        }
        BehaviorAction::EscapeObstacle(action) => {
            insert_action(world, actor, instruction, RunBehaviorEscapeObstacle(action))
        }
        BehaviorAction::ExitInteractionContainer { exit_behavior_set } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorExitInteractionContainer(exit_behavior_set),
        ),
        BehaviorAction::FindArtifact => {
            insert_action(world, actor, instruction, RunBehaviorFindArtifact)
        }
        BehaviorAction::DigArtifact => {
            insert_action(world, actor, instruction, RunBehaviorDigArtifact)
        }
        BehaviorAction::Tour => insert_action(world, actor, instruction, RunBehaviorTour),
        BehaviorAction::TeleportToLocation(action) => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorTeleportToLocation(action),
        ),
        BehaviorAction::KickOffRider => {
            insert_action(world, actor, instruction, RunBehaviorKickOffRider)
        }
        BehaviorAction::AnimateForViewFromTransport { animation_type } => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorAnimateForViewFromTransport(animation_type),
        ),
        BehaviorAction::FollowEntity(action) => {
            insert_action(world, actor, instruction, RunBehaviorFollowEntity(action))
        }
        BehaviorAction::SplashGuest(action) => {
            insert_action(world, actor, instruction, RunBehaviorSplashGuest(action))
        }
        BehaviorAction::UsePortal(action) => {
            insert_action(world, actor, instruction, RunBehaviorUsePortal(action))
        }
        BehaviorAction::DockTankWall(action) => {
            insert_action(world, actor, instruction, RunBehaviorDockTankWall(action))
        }
        BehaviorAction::StaffTrainAnimal(action) => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorStaffTrainAnimal(action),
        ),
        BehaviorAction::AddSubjectAsDiseaseTarget(action) => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorAddSubjectAsDiseaseTarget(action),
        ),
        BehaviorAction::InvalidAuthoredAction(action) => insert_action(
            world,
            actor,
            instruction,
            RunBehaviorInvalidAuthoredAction(action),
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
    let mut actor = world.entity_mut(actor);
    let id = if let Some(mut generation) = actor.get_mut::<BehaviorInstructionGeneration>() {
        generation.0 = generation
            .0
            .checked_add(1)
            .expect("behavior instruction identity exhausted");
        BehaviorInstructionId(generation.0)
    } else {
        actor.insert(BehaviorInstructionGeneration(1));
        BehaviorInstructionId(1)
    };
    actor.insert((
        RunningBehaviorInstruction {
            id,
            execution_id,
            origin,
            depth,
            component,
            reservation,
            activated_at: None,
        },
        action,
    ));
}
