use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// The show or break row selected by a scheduler UI controller.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = ShowScheduleSelectors)]
pub struct SelectedShowScheduleRow(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = SelectedShowScheduleRow)]
pub struct ShowScheduleSelectors(Vec<Entity>);

/// Stable authored order of one show or break in its stage's schedule.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ShowScheduleRowOrder(pub u16);

/// Links an ordered show or break row to its physical stage.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = ShowStageRows)]
#[require(ScheduledShowRating)]
pub struct ScheduledShowRow {
    pub stage: Entity,
}

#[derive(Component)]
#[relationship_target(relationship = ScheduledShowRow, linked_spawn)]
pub struct ShowStageRows(Vec<Entity>);

/// The weighted rating retained between performances of this scheduled show.
#[derive(Component, Default)]
pub struct ScheduledShowRating(pub Option<f32>);

/// Whether one authored scheduled show row is enabled.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduledShowRowActivationState {
    Enabled,
    Disabled,
}

/// An explicit rest interval in a stage schedule.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledShowBreakRow {
    pub duration_ticks: u32,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledShowPerformancePlan {
    pub trick_performance_capacity: u16,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledShowTrickPerformance {
    pub trick: AssetId,
    pub duration_ticks: u32,
}

#[derive(Component)]
#[relationship(relationship_target = ShowTrickSlots)]
pub struct ShowTrickSlotOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ShowTrickSlotOf, linked_spawn)]
pub struct ShowTrickSlots(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = ScheduledPerformerTricks)]
pub struct ScheduledTrickPerformer(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ScheduledTrickPerformer, linked_spawn)]
pub struct ScheduledPerformerTricks(Vec<Entity>);
