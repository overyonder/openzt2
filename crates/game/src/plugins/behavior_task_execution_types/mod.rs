use arrayvec::ArrayVec;
use bevy::prelude::*;
use openzt2_game_data::behavior::action_record::BehaviorAction;
use openzt2_game_data::AssetId;

use crate::assets::behavior::behavior_asset_types::BehaviorDocumentAsset;

pub(crate) const MAXIMUM_BEHAVIOR_TASK_RETURN_FRAME_COUNT: usize = 16;

/// Bevy-owned identity for one root task execution, including nested sets.
#[derive(Resource, Default)]
pub(crate) struct BehaviorTaskExecutionSequence(u64);

impl BehaviorTaskExecutionSequence {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .checked_add(1)
            .expect("behavior execution identity exhausted");
        self.0
    }
}

#[derive(Component, Clone, Debug)]
pub(crate) struct BehaviorTaskExecutionState {
    pub(crate) execution_id: u64,
    pub(crate) program: AssetId,
    pub(crate) target: Option<Entity>,
    pub(crate) document: Handle<BehaviorDocumentAsset>,
    pub(crate) declaration: usize,
    pub(crate) phase: BehaviorTaskExecutionPhase,
    pub(crate) action: usize,
    /// Random-set remaining plays plus one; zero means this action has not started.
    pub(crate) repetitions: u32,
    pub(crate) next_action_tick: u64,
    /// Primitive index into the current target object's authored
    /// interaction-slot definitions.
    pub(crate) interaction_slot: Option<usize>,
    pub(crate) stack: ArrayVec<BehaviorTaskReturnFrame, MAXIMUM_BEHAVIOR_TASK_RETURN_FRAME_COUNT>,
}

impl BehaviorTaskExecutionState {
    pub(crate) fn is_at_or_nested_under(
        &self,
        origin: &BehaviorTaskReturnFrame,
        depth: usize,
    ) -> bool {
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

    pub(crate) fn restore_suspended_instruction(
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

    pub(crate) const fn program(&self) -> AssetId {
        self.program
    }

    pub(crate) const fn target(&self) -> Option<Entity> {
        self.target
    }

    pub(crate) fn create_return_frame_after_action(
        &self,
        action_index: usize,
    ) -> BehaviorTaskReturnFrame {
        BehaviorTaskReturnFrame {
            program: self.program,
            document: self.document.clone(),
            declaration: self.declaration,
            phase: self.phase,
            action: action_index,
            repetitions: self.repetitions,
        }
    }

    pub(crate) fn push_return_frame(&mut self, frame: BehaviorTaskReturnFrame) -> bool {
        self.stack.try_push(frame).is_ok()
    }

    /// Nested behavior sets retain the enclosing task's container selection.
    pub(crate) fn reservation_tag(
        &self,
        documents: &Assets<BehaviorDocumentAsset>,
    ) -> Option<AssetId> {
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
pub(crate) struct BehaviorTaskReturnFrame {
    pub(crate) program: AssetId,
    pub(crate) document: Handle<BehaviorDocumentAsset>,
    pub(crate) declaration: usize,
    pub(crate) phase: BehaviorTaskExecutionPhase,
    pub(crate) action: usize,
    pub(crate) repetitions: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BehaviorTaskExecutionPhase {
    Set,
    Execution,
    Completion,
    Failure,
}

#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct PendingBehaviorTaskFailure;

#[derive(Component, Clone, Debug)]
pub(crate) struct PendingBehaviorAnimationClipCompletion {
    pub(crate) execution_id: u64,
    pub(crate) request_id: usize,
    pub(crate) origin: BehaviorTaskReturnFrame,
    pub(crate) stack_depth: usize,
    pub(crate) target: Option<Entity>,
    /// Index into the current authored random-animation choices; ordinary clips
    /// use their action directly and retain no copied clip name.
    pub(crate) random_choice: Option<usize>,
}

impl PendingBehaviorAnimationClipCompletion {
    pub(crate) fn still_owns_exact_task_frame(&self, task: &BehaviorTaskExecutionState) -> bool {
        task.execution_id == self.execution_id
            && task.target == self.target
            && task.stack.len() == self.stack_depth
            && task.is_at_or_nested_under(&self.origin, self.stack_depth)
    }
    pub(crate) fn clip_key<'a>(&self, action: &'a BehaviorAction) -> Option<&'a str> {
        match action {
            BehaviorAction::AnimationClip(clip) if self.random_choice.is_none() => {
                Some(&clip.animation_clip_asset_key)
            }
            BehaviorAction::RandomAnimation { weighted_clips, .. } => weighted_clips
                .get(self.random_choice?)
                .map(|(key, _)| key.as_str()),
            _ => None,
        }
    }
}

#[derive(Component, Clone, Debug)]
pub(crate) struct PendingBehaviorDockingCompletion {
    pub(crate) target: Entity,
    pub(crate) origin: BehaviorTaskReturnFrame,
    pub(crate) stack_depth: usize,
    pub(crate) request_id: u64,
}

impl PendingBehaviorDockingCompletion {
    pub(crate) fn still_owns_exact_task_frame(&self, task: &BehaviorTaskExecutionState) -> bool {
        task.target() == Some(self.target)
            && task.stack.len() == self.stack_depth
            && task.is_at_or_nested_under(&self.origin, self.stack_depth)
    }
}

#[derive(Message, Clone, Debug)]
pub(crate) struct BehaviorFeedbackDispatched {
    pub(crate) actor: Entity,
    pub(crate) document: Handle<BehaviorDocumentAsset>,
    pub(crate) declaration: usize,
    pub(crate) phase: BehaviorTaskExecutionPhase,
    pub(crate) action: usize,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BehaviorTaskFinished {
    pub(crate) actor: Entity,
    pub(crate) execution_id: u64,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BehaviorTaskFailed {
    pub(crate) actor: Entity,
    pub(crate) execution_id: u64,
}

pub(crate) fn find_current_behavior_task_action<'a>(
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

pub(crate) fn advance_behavior_task_to_next_action(
    behavior_task: &mut BehaviorTaskExecutionState,
    current_simulation_tick: u64,
) {
    behavior_task.action = behavior_task.action.saturating_add(1);
    behavior_task.repetitions = 0;
    behavior_task.next_action_tick = current_simulation_tick.saturating_add(1);
}
