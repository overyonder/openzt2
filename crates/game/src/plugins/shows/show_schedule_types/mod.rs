use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// The show or break row selected by a scheduler UI controller.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelectedShowScheduleRow(pub(crate) Entity);

/// Stable authored order of one show or break in its stage's schedule.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ShowScheduleRowOrder(pub(crate) u16);

/// A scheduled show belongs to one physical stage. Its
/// `ScheduledShowPerformancePlan` carries the performer/trick sequence edited
/// by the mixer.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScheduledShowRow {
    pub(crate) stage: Entity,
}

/// Whether one authored scheduled show row is enabled.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScheduledShowRowActivationState {
    Enabled,
    Disabled,
}

/// An explicit rest interval in a stage schedule.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScheduledShowBreakRow {
    pub(crate) stage: Entity,
    pub(crate) duration_ticks: u32,
}

/// The ordered performer/trick sequence owned by one scheduled show row.
/// Capacity is fixed at creation so editing never reallocates the sequence.
#[derive(Component, Debug, Clone, PartialEq)]
pub(crate) struct ScheduledShowPerformancePlan {
    pub(crate) trick_performances: Vec<ScheduledShowTrickPerformance>,
    pub(crate) trick_performance_capacity: u16,
}

impl ScheduledShowPerformancePlan {
    pub(crate) fn with_trick_performance_capacity(trick_performance_capacity: u16) -> Self {
        Self {
            trick_performances: Vec::with_capacity(usize::from(trick_performance_capacity)),
            trick_performance_capacity,
        }
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.trick_performances.len() <= usize::from(self.trick_performance_capacity)
            && self.trick_performances.capacity() >= usize::from(self.trick_performance_capacity)
            && self
                .trick_performances
                .iter()
                .all(|performance| performance.duration_ticks > 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScheduledShowTrickPerformance {
    pub(crate) performer: Entity,
    pub(crate) trick: AssetId,
    pub(crate) duration_ticks: u32,
}
