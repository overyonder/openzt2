use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// The show or break row selected by a scheduler UI controller.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedShowScheduleRow(pub Entity);

/// Stable authored order of one show or break in its stage's schedule.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ShowScheduleRowOrder(pub u16);

/// A scheduled show belongs to one physical stage. Its
/// `ScheduledShowPerformancePlan` carries the performer/trick sequence edited
/// by the mixer.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledShowRow {
    pub stage: Entity,
}

/// Whether one authored scheduled show row is enabled.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledShowRowActivationState {
    Enabled,
    Disabled,
}

/// An explicit rest interval in a stage schedule.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledShowBreakRow {
    pub stage: Entity,
    pub duration_ticks: u32,
}

/// The ordered performer/trick sequence owned by one scheduled show row.
/// Capacity is fixed at creation so editing never reallocates the sequence.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct ScheduledShowPerformancePlan {
    pub trick_performances: Vec<ScheduledShowTrickPerformance>,
    pub trick_performance_capacity: u16,
}

impl ScheduledShowPerformancePlan {
    pub fn with_trick_performance_capacity(trick_performance_capacity: u16) -> Self {
        Self {
            trick_performances: Vec::with_capacity(usize::from(trick_performance_capacity)),
            trick_performance_capacity,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.trick_performances.len() <= usize::from(self.trick_performance_capacity)
            && self.trick_performances.capacity() >= usize::from(self.trick_performance_capacity)
            && self
                .trick_performances
                .iter()
                .all(|performance| performance.duration_ticks > 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledShowTrickPerformance {
    pub performer: Entity,
    pub trick: AssetId,
    pub duration_ticks: u32,
}
