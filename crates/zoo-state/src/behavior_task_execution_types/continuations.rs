use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::{BehaviorTaskExecutionState, BehaviorTaskReturnFrame};

#[derive(Component)]
#[relationship(relationship_target = BehaviorContinuations)]
pub struct BehaviorContinuationOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = BehaviorContinuationOf, linked_spawn)]
pub struct BehaviorContinuations(Vec<Entity>);

#[derive(Component)]
pub struct SuspendedBehaviorFrame(pub BehaviorTaskReturnFrame);

/// The target a suspended instruction had when the set it entered was given a
/// target of its own. However the set ends, the instruction resumes with it.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuspendedBehaviorTarget(pub Option<Entity>);

#[derive(SystemParam)]
pub struct BehaviorContinuationFrames<'w, 's> {
    continuations: Query<'w, 's, &'static BehaviorContinuations>,
    frames: Query<'w, 's, &'static SuspendedBehaviorFrame>,
    targets: Query<'w, 's, &'static SuspendedBehaviorTarget>,
}

impl BehaviorContinuationFrames<'_, '_> {
    pub fn frame_entities(&self, actor: Entity) -> impl DoubleEndedIterator<Item = Entity> + '_ {
        self.continuations
            .get(actor)
            .into_iter()
            .flat_map(|c| c.0.iter().copied())
    }

    pub fn frames(
        &self,
        actor: Entity,
    ) -> impl DoubleEndedIterator<Item = &BehaviorTaskReturnFrame> + '_ {
        self.frame_entities(actor)
            .filter_map(|frame| self.frames.get(frame).ok().map(|frame| &frame.0))
    }

    /// Each suspended frame with the target its instruction resumes with, if
    /// the set it entered had a target of its own.
    pub fn frames_with_targets(
        &self,
        actor: Entity,
    ) -> impl DoubleEndedIterator<Item = (&BehaviorTaskReturnFrame, Option<Option<Entity>>)> + '_
    {
        self.frame_entities(actor).filter_map(|frame| {
            let target = self.targets.get(frame).ok().map(|target| target.0);
            self.frames.get(frame).ok().map(|frame| (&frame.0, target))
        })
    }

    /// The target the instruction suspended at `depth` resumes with, if the
    /// set it entered had a target of its own.
    pub fn suspended_target(&self, actor: Entity, depth: usize) -> Option<Option<Entity>> {
        let frame = self.frame_entities(actor).nth(depth)?;
        self.targets.get(frame).ok().map(|target| target.0)
    }

    pub fn owns(
        &self,
        actor: Entity,
        task: &BehaviorTaskExecutionState,
        origin: &BehaviorTaskReturnFrame,
        depth: usize,
    ) -> bool {
        if task.depth == depth {
            task.is_at_exact_frame(origin, depth)
        } else {
            task.depth > depth
                && self
                    .frames(actor)
                    .nth(depth)
                    .is_some_and(|frame| frame.same_instruction(origin))
        }
    }

    /// Removes the innermost suspended frame, returning it with the target its
    /// instruction resumes with when the set it entered had its own.
    pub fn pop(
        &self,
        actor: Entity,
        commands: &mut Commands,
    ) -> Option<(BehaviorTaskReturnFrame, Option<Option<Entity>>)> {
        let entity = self.frame_entities(actor).next_back()?;
        let frame = self.frames.get(entity).ok()?;
        let target = self.targets.get(entity).ok().map(|target| target.0);
        commands.entity(entity).try_despawn();
        Some((frame.0.clone(), target))
    }

    pub fn truncate(&self, actor: Entity, depth: usize, commands: &mut Commands) {
        let Some(continuations) = self.continuations.get(actor).ok() else {
            return;
        };
        let remove = continuations.0.len().saturating_sub(depth);
        for entity in continuations.0.iter().copied().rev().take(remove) {
            commands.entity(entity).try_despawn();
        }
    }
}

pub fn suspend_behavior_frame(
    actor: Entity,
    frame: BehaviorTaskReturnFrame,
    target: Option<Option<Entity>>,
    commands: &mut Commands,
) {
    let mut suspended =
        commands.spawn((BehaviorContinuationOf(actor), SuspendedBehaviorFrame(frame)));
    if let Some(target) = target {
        suspended.insert(SuspendedBehaviorTarget(target));
    }
}
