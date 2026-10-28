//! Effective score of an actor's running task.

use bevy::prelude::*;

use crate::behavior_task_execution_types::BehaviorTaskExecutionState;

/// Interruption priority of the root task and its linked request payload.
/// Nested sets retain this record; replacing the root execution discards it.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct BehaviorTaskEffectiveScore {
    pub execution_id: u64,
    pub template_score: Option<f32>,
    pub linked_score: Option<f32>,
    pub selected_candidate_score: Option<f32>,
}

impl BehaviorTaskEffectiveScore {
    /// Scores recorded for an earlier execution no longer apply.
    pub fn of_running_task(score: Option<&Self>, task: &BehaviorTaskExecutionState) -> f32 {
        score
            .filter(|score| score.execution_id == task.execution_id)
            .map_or(0.0, Self::effective_score)
    }

    pub fn effective_score(&self) -> f32 {
        let Some(template) = self.template_score else {
            return self
                .linked_score
                .filter(|score| *score > 0.0)
                .unwrap_or(0.0);
        };
        self.linked_score.map_or(
            template,
            |linked| {
                if linked > template {
                    linked
                } else {
                    template
                }
            },
        )
    }
}
