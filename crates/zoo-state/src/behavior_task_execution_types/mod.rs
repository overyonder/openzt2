use crate::animal_behavior::behavior_synchronized_set_types::SynchronizedBehaviorPartner;
use bevy::prelude::*;
use openzt2_game_data::behavior::action_record::BehaviorAction;
use openzt2_game_data::AssetId;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;

pub mod continuations;
pub mod running_actions;

pub const MAXIMUM_BEHAVIOR_TASK_RETURN_FRAME_COUNT: usize = 16;

/// A dock owns a stationary authored pose while measuring its attachment point.
#[derive(Component)]
pub struct SamplingBehaviorDockPose;

/// Bevy-owned identity for one root task execution, including nested sets.
#[derive(Resource, Default)]
pub struct BehaviorTaskExecutionSequence(u64);

impl BehaviorTaskExecutionSequence {
    pub const fn from_snapshot(last_issued: u64) -> Self {
        Self(last_issued)
    }

    pub const fn last_issued(&self) -> u64 {
        self.0
    }

    pub fn reserve_restored(&mut self, execution_id: u64) {
        self.0 = self.0.max(execution_id);
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .checked_add(1)
            .expect("behavior execution identity exhausted");
        self.0
    }
}

#[derive(Component, Clone, Debug)]
#[component(
    on_insert = running_actions::initialize_running_behavior_instruction,
    on_discard = running_actions::discard_running_behavior_instruction
)]
pub struct BehaviorTaskExecutionState {
    pub execution_id: u64,
    pub program: AssetId,
    pub target: Option<Entity>,
    /// The item the task uses, such as the hamburger a guest eats. It is one
    /// of the subject's held or inventory items named by the task's `Objects`.
    pub object: Option<Entity>,
    pub document: Handle<BehaviorDocumentAsset>,
    pub declaration: usize,
    pub phase: BehaviorTaskExecutionPhase,
    pub action: usize,
    /// Random-set remaining plays plus one; zero means this action has not started.
    pub repetitions: u32,
    pub next_action_tick: u64,
    /// Primitive index into the current target object's authored
    /// interaction-slot definitions.
    pub interaction_slot: Option<usize>,
    pub depth: usize,
}

impl BehaviorTaskExecutionState {
    pub fn is_at_exact_frame(&self, origin: &BehaviorTaskReturnFrame, depth: usize) -> bool {
        self.depth == depth
            && self.program == origin.program
            && self.document == origin.document
            && self.declaration == origin.declaration
            && self.phase == origin.phase
            && self.action == origin.action
    }

    pub fn restore_suspended_instruction(
        &mut self,
        origin: &BehaviorTaskReturnFrame,
        depth: usize,
        tick: u64,
    ) {
        self.depth = depth;
        self.program = origin.program;
        self.document = origin.document.clone();
        self.declaration = origin.declaration;
        self.phase = origin.phase;
        self.action = origin.action;
        self.repetitions = origin.repetitions;
        self.next_action_tick = tick;
    }

    pub const fn program(&self) -> AssetId {
        self.program
    }

    pub const fn target(&self) -> Option<Entity> {
        self.target
    }

    pub fn create_return_frame_after_action(&self, action_index: usize) -> BehaviorTaskReturnFrame {
        BehaviorTaskReturnFrame {
            program: self.program,
            document: self.document.clone(),
            declaration: self.declaration,
            phase: self.phase,
            action: action_index,
            repetitions: self.repetitions,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BehaviorTaskReturnFrame {
    pub program: AssetId,
    pub document: Handle<BehaviorDocumentAsset>,
    pub declaration: usize,
    pub phase: BehaviorTaskExecutionPhase,
    pub action: usize,
    pub repetitions: u32,
}

impl BehaviorTaskReturnFrame {
    pub fn same_instruction(&self, other: &Self) -> bool {
        self.program == other.program
            && self.document == other.document
            && self.declaration == other.declaration
            && self.phase == other.phase
            && self.action == other.action
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BehaviorTaskExecutionPhase {
    Set,
    Execution,
    Completion,
    Failure,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct PendingBehaviorTaskFailure;

/// A target that `BFBehKill killTarget` removed. Later actions of the same
/// execution can still read it, as `CompostPoo.lua` reads the raked waste.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BehaviorKilledTarget {
    pub execution_id: u64,
    pub entity: Entity,
    pub definition: AssetId,
    pub translation: Vec3,
}

/// The authored kill consumed a litter job's target even if a later step fails.
#[derive(Component)]
pub struct AppliedBehaviorLitterKill(pub u64);

/// An explicit clip that a behavior action started and does not wait for,
/// such as lifting an item until its attach key fires. Locomotion leaves the
/// actor's presentation to it until that request completes, is rejected or
/// is replaced by another explicit request.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BehaviorHeldAnimationClip {
    pub request_id: usize,
}

/// Drops the actor's current task without its failure branch, so the next
/// selection chooses afresh. Each executor's pending state names its owning
/// execution and is discarded once that execution is gone.
pub fn abandon_behavior_task_for_reconsideration(actor: Entity, commands: &mut Commands) {
    commands.entity(actor).try_remove::<(
        BehaviorTaskExecutionState,
        crate::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore,
        PendingBehaviorTaskFailure,
        PendingBehaviorAnimationClipCompletion,
        PendingBehaviorDockingCompletion,
        PendingBehaviorMoveCompletion,
        crate::locomotion::locomotion_types::Destination,
        crate::locomotion::locomotion_types::Docking,
    )>();
}

#[derive(Component, Clone, Debug)]
pub struct PendingBehaviorAnimationClipCompletion {
    pub instruction: running_actions::BehaviorInstructionId,
    pub request_id: usize,
    pub clip_key: String,
    pub target: Option<Entity>,
    pub repeats: bool,
}

impl PendingBehaviorAnimationClipCompletion {
    pub fn owns(
        &self,
        task: &BehaviorTaskExecutionState,
        instruction: &running_actions::RunningBehaviorInstruction,
    ) -> bool {
        self.instruction == instruction.id()
            && task.target == self.target
            && instruction.is_current(task)
    }
}

#[derive(Component, Clone, Debug)]
pub struct PendingBehaviorDockingCompletion {
    pub target: Entity,
    pub instruction: running_actions::BehaviorInstructionId,
    pub request_id: u64,
}

impl PendingBehaviorDockingCompletion {
    pub fn owns(
        &self,
        task: &BehaviorTaskExecutionState,
        instruction: &running_actions::RunningBehaviorInstruction,
        target: Option<Entity>,
    ) -> bool {
        target == Some(self.target)
            && self.instruction == instruction.id()
            && instruction.is_current(task)
    }
}

#[derive(Message, Clone, Debug)]
pub struct BehaviorFeedbackDispatched {
    pub actor: Entity,
    pub document: Handle<BehaviorDocumentAsset>,
    pub declaration: usize,
    pub phase: BehaviorTaskExecutionPhase,
    pub action: usize,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BehaviorTaskFinished {
    pub actor: Entity,
    pub execution_id: u64,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BehaviorTaskFailed {
    pub actor: Entity,
    pub execution_id: u64,
}

impl BehaviorTaskExecutionPhase {
    pub fn authored_actions(
        self,
        document: &BehaviorDocumentAsset,
        declaration: usize,
    ) -> Option<&openzt2_game_data::behavior::document::BehaviorTaskActionPhase> {
        match self {
            Self::Set => Some(&document.behavior_set_at_index(declaration)?.actions),
            Self::Execution => Some(&document.behavior_task_at_index(declaration)?.execution),
            Self::Completion => Some(&document.behavior_task_at_index(declaration)?.completion),
            Self::Failure => Some(&document.behavior_task_at_index(declaration)?.failure),
        }
    }

    pub fn authored_action(
        self,
        document: &BehaviorDocumentAsset,
        declaration: usize,
        index: usize,
    ) -> Option<&BehaviorAction> {
        self.authored_actions(document, declaration)?
            .lowered_actions()
            .get(index)
    }
}

pub fn advance_behavior_task_to_next_action(
    behavior_task: &mut BehaviorTaskExecutionState,
    current_simulation_tick: u64,
) {
    behavior_task.action = behavior_task.action.saturating_add(1);
    behavior_task.repetitions = 0;
    behavior_task.next_action_tick = current_simulation_tick;
}

#[derive(Component, Clone, Debug)]
pub struct PendingBehaviorMoveCompletion {
    pub instruction: running_actions::BehaviorInstructionId,
    pub request_id: u64,
}

/// An incoming synchronization claims the partner until its operation ends.
#[derive(Component)]
#[relationship_target(relationship = SynchronizedBehaviorPartner)]
pub struct SynchronizedBehaviorParticipant(Entity);

/// A live evasion instruction; navigation combines this horizontal steering
/// direction with its obstacle and contact response each fixed step.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BehaviorEvasionSteering {
    pub request_id: u64,
    pub direction: Vec2,
    pub obstacle_radius_m: f32,
    pub avoid_water: bool,
    pub avoid_land: bool,
}

#[derive(Component)]
pub struct BehaviorWanderNavigation {
    pub instruction: running_actions::BehaviorInstructionId,
    pub until_tick: u64,
    pub request_id: Option<u64>,
}

#[derive(Component)]
pub struct BehaviorEvasionNavigation {
    pub instruction: running_actions::BehaviorInstructionId,
    pub until_tick: u64,
    pub request_id: u64,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct BehaviorLocomotionModifier {
    pub modifier: openzt2_game_data::AssetId,
    pub item: Entity,
}

impl PendingBehaviorMoveCompletion {
    pub fn owns(
        &self,
        task: &BehaviorTaskExecutionState,
        instruction: &running_actions::RunningBehaviorInstruction,
    ) -> bool {
        self.instruction == instruction.id() && instruction.is_current(task)
    }
}
