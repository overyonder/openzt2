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

#[derive(SystemParam)]
pub struct BehaviorContinuationFrames<'w, 's> {
    continuations: Query<'w, 's, &'static BehaviorContinuations>,
    frames: Query<'w, 's, &'static SuspendedBehaviorFrame>,
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

    pub fn pop(&self, actor: Entity, commands: &mut Commands) -> Option<BehaviorTaskReturnFrame> {
        let entity = self.frame_entities(actor).next_back()?;
        let frame = self.frames.get(entity).ok()?;
        commands.entity(entity).try_despawn();
        Some(frame.0.clone())
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
    commands: &mut Commands,
) {
    commands.spawn((BehaviorContinuationOf(actor), SuspendedBehaviorFrame(frame)));
}
