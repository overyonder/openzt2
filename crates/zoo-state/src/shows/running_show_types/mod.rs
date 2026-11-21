use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component)]
#[relationship(relationship_target = StageRunningShow)]
pub struct RunningShowOnStage(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = RunningShowOnStage, linked_spawn)]
pub struct StageRunningShow(Entity);

/// Links a viewing location outside the stage's own hierarchy to the stage it serves.
#[derive(Component)]
#[relationship(relationship_target = ShowViewingLocations)]
pub struct ShowViewingLocationOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ShowViewingLocationOf)]
pub struct ShowViewingLocations(Vec<Entity>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowPhase {
    Preshow,
    Conducting,
    Postshow,
    BetweenShows,
}

#[derive(Component)]
pub struct RunningShow {
    pub phase: ShowPhase,
    pub remaining_ticks: u32,
    pub current_slot: usize,
    pub trick_quality_q16: i32,
    pub completed_tricks: u32,
}

#[derive(Component)]
#[relationship(relationship_target = RunningShowsForScheduleRow)]
pub struct RunningShowFromRow(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = RunningShowFromRow, linked_spawn)]
pub struct RunningShowsForScheduleRow(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = AnimalPerformingShowTrick)]
pub struct ShowTrickBeingPerformedBy(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ShowTrickBeingPerformedBy)]
pub struct AnimalPerformingShowTrick(Entity);

#[derive(Component)]
pub struct RunningShowTrick {
    pub definition: AssetId,
    pub slot: Entity,
    pub scored: bool,
}

/// Published when the show reaches a scheduled trick. The performer owns the
/// behavior that executes it and publishes its outcome.
#[derive(Message, Clone, Copy)]
pub struct BeginScheduledShowTrick {
    pub show: Entity,
    pub stage: Entity,
    pub performer: Entity,
    pub slot: Entity,
    pub trick: AssetId,
    pub duration_ticks: u32,
}

#[derive(Message, Clone, Copy)]
pub struct ShowAudienceReaction {
    pub guest: Entity,
    pub show: Entity,
    pub stage: Entity,
    pub event: AssetId,
    pub score_q16: i32,
}

/// The performer's scored outcome for the trick it was assigned.
#[derive(Message, Clone, Copy)]
pub struct ScheduledShowTrickScored {
    pub show: Entity,
    pub performer: Entity,
    pub slot: Entity,
    pub trick: AssetId,
    pub score_q16: i32,
}

#[derive(Message, Clone, Copy)]
pub struct ShowRunFinished {
    pub stage: Entity,
}
