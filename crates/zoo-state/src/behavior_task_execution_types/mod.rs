use arrayvec::ArrayVec;
use bevy::prelude::*;
use openzt2_game_data::behavior::action_record::BehaviorAction;
use openzt2_game_data::AssetId;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;

pub const MAXIMUM_BEHAVIOR_TASK_RETURN_FRAME_COUNT: usize = 16;

/// A dock owns a stationary authored pose while measuring its attachment point.
#[derive(Component)]
pub struct SamplingBehaviorDockPose;

/// Bevy-owned identity for one root task execution, including nested sets.
#[derive(Resource, Default)]
pub struct BehaviorTaskExecutionSequence(u64);

impl BehaviorTaskExecutionSequence {
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
    pub stack: ArrayVec<BehaviorTaskReturnFrame, MAXIMUM_BEHAVIOR_TASK_RETURN_FRAME_COUNT>,
}

impl BehaviorTaskExecutionState {
    pub fn is_at_or_nested_under(&self, origin: &BehaviorTaskReturnFrame, depth: usize) -> bool {
        let matches = |frame: &BehaviorTaskReturnFrame| {
            frame.program == origin.program
                && frame.document == origin.document
                && frame.declaration == origin.declaration
                && frame.phase == origin.phase
                && frame.action == origin.action
        };
        if self.stack.len() == depth {
            self.program == origin.program
                && self.document == origin.document
                && self.declaration == origin.declaration
                && self.phase == origin.phase
                && self.action == origin.action
        } else {
            self.stack.get(depth).is_some_and(matches)
        }
    }

    pub fn restore_suspended_instruction(
        &mut self,
        origin: &BehaviorTaskReturnFrame,
        depth: usize,
        tick: u64,
    ) {
        self.stack.truncate(depth);
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

    pub fn push_return_frame(&mut self, frame: BehaviorTaskReturnFrame) -> bool {
        self.stack.try_push(frame).is_ok()
    }

    /// Nested behavior sets retain the enclosing task's container selection.
    pub fn reservation_tag(&self, documents: &Assets<BehaviorDocumentAsset>) -> Option<AssetId> {
        std::iter::once((&self.document, self.declaration, self.phase))
            .chain(
                self.stack
                    .iter()
                    .rev()
                    .map(|frame| (&frame.document, frame.declaration, frame.phase)),
            )
            .find_map(|(document, declaration, phase)| {
                (phase != BehaviorTaskExecutionPhase::Set)
                    .then(|| {
                        documents
                            .get(document)?
                            .behavior_task_at_index(declaration)?
                            .reservation_tag
                    })
                    .flatten()
            })
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BehaviorTaskExecutionPhase {
    Set,
    Execution,
    Completion,
    Failure,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct PendingBehaviorTaskFailure;

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
    pub execution_id: u64,
    pub request_id: usize,
    pub origin: BehaviorTaskReturnFrame,
    pub stack_depth: usize,
    pub target: Option<Entity>,
    /// Index into the current authored random-animation choices; ordinary clips
    /// use their action directly and retain no copied clip name.
    pub random_choice: Option<usize>,
}

impl PendingBehaviorAnimationClipCompletion {
    pub fn still_owns_exact_task_frame(&self, task: &BehaviorTaskExecutionState) -> bool {
        task.execution_id == self.execution_id
            && task.target == self.target
            && task.stack.len() == self.stack_depth
            && task.is_at_or_nested_under(&self.origin, self.stack_depth)
    }
    pub fn clip_key<'a>(&self, action: &'a BehaviorAction) -> Option<&'a str> {
        match action {
            BehaviorAction::AnimationClip(clip) if self.random_choice.is_none() => {
                Some(&clip.animation_clip_asset_key)
            }
            BehaviorAction::RandomAnimation { weighted_clips, .. } => weighted_clips
                .get(self.random_choice?)
                .map(|(key, _)| key.as_str()),
            // A target that holds no prop leaves only the attachment's clip.
            BehaviorAction::AttachObject(attach)
                if self.random_choice.is_none() && attach.attaches_target_held_prop() =>
            {
                attach.animation.as_deref()
            }
            _ => None,
        }
    }
}

#[derive(Component, Clone, Debug)]
pub struct PendingBehaviorDockingCompletion {
    pub target: Entity,
    pub origin: BehaviorTaskReturnFrame,
    pub stack_depth: usize,
    pub request_id: u64,
}

impl PendingBehaviorDockingCompletion {
    pub fn still_owns_exact_task_frame(&self, task: &BehaviorTaskExecutionState) -> bool {
        task.target() == Some(self.target)
            && task.stack.len() == self.stack_depth
            && task.is_at_or_nested_under(&self.origin, self.stack_depth)
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

pub fn find_current_behavior_task_action<'a>(
    behavior_document_asset: &'a BehaviorDocumentAsset,
    behavior_task: &BehaviorTaskExecutionState,
) -> Option<&'a BehaviorAction> {
    match behavior_task.phase {
        BehaviorTaskExecutionPhase::Set => behavior_document_asset
            .behavior_set_at_index(behavior_task.declaration)?
            .actions
            .lowered_actions()
            .get(behavior_task.action),
        BehaviorTaskExecutionPhase::Execution => behavior_document_asset
            .behavior_task_at_index(behavior_task.declaration)?
            .execution
            .lowered_actions()
            .get(behavior_task.action),
        BehaviorTaskExecutionPhase::Completion => behavior_document_asset
            .behavior_task_at_index(behavior_task.declaration)?
            .completion
            .lowered_actions()
            .get(behavior_task.action),
        BehaviorTaskExecutionPhase::Failure => behavior_document_asset
            .behavior_task_at_index(behavior_task.declaration)?
            .failure
            .lowered_actions()
            .get(behavior_task.action),
    }
}

pub fn advance_behavior_task_to_next_action(
    behavior_task: &mut BehaviorTaskExecutionState,
    current_simulation_tick: u64,
) {
    behavior_task.action = behavior_task.action.saturating_add(1);
    behavior_task.repetitions = 0;
    behavior_task.next_action_tick = current_simulation_tick.saturating_add(1);
}

#[derive(Component, Clone, Debug)]
pub struct PendingBehaviorMoveCompletion {
    pub origin: BehaviorTaskReturnFrame,
    pub stack_depth: usize,
    pub request_id: u64,
}

/// While coupled, ordinary set requests must not replace the target's task.
#[derive(Component)]
pub struct SynchronizedBehaviorParticipant {
    pub execution_id: u64,
    pub owner: Entity,
}

/// A live evasion instruction; navigation combines this horizontal steering
/// direction with its obstacle and contact response each fixed step.
#[derive(Component, Clone, Copy, Debug)]
pub struct BehaviorEvasionSteering {
    pub request_id: u64,
    pub direction: Vec2,
    pub obstacle_radius_m: f32,
    pub avoid_water: bool,
    pub avoid_land: bool,
}

#[derive(Component)]
pub struct BehaviorWanderNavigation {
    pub execution_id: u64,
    pub origin: BehaviorTaskReturnFrame,
    pub depth: usize,
    pub until_tick: u64,
    pub request_id: Option<u64>,
}

impl BehaviorWanderNavigation {
    pub fn owns(&self, task: &BehaviorTaskExecutionState) -> bool {
        self.execution_id == task.execution_id
            && self.depth == task.stack.len()
            && task.is_at_or_nested_under(&self.origin, self.depth)
    }
}

#[derive(Component)]
pub struct BehaviorEvasionNavigation {
    pub execution_id: u64,
    pub origin: BehaviorTaskReturnFrame,
    pub depth: usize,
    pub until_tick: u64,
    pub request_id: u64,
}

impl BehaviorEvasionNavigation {
    pub fn owns(&self, task: &BehaviorTaskExecutionState) -> bool {
        self.execution_id == task.execution_id
            && self.depth == task.stack.len()
            && task.is_at_or_nested_under(&self.origin, self.depth)
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct BehaviorLocomotionModifier {
    pub modifier: openzt2_game_data::AssetId,
    pub item: Entity,
}

impl PendingBehaviorMoveCompletion {
    pub fn still_owns_exact_task_frame(&self, task: &BehaviorTaskExecutionState) -> bool {
        task.stack.len() == self.stack_depth
            && task.is_at_or_nested_under(&self.origin, self.stack_depth)
    }
}
