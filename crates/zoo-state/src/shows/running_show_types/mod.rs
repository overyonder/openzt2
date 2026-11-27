use bevy::{platform::collections::HashMap, prelude::*};
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
#[relationship_target(relationship = ShowViewingLocationOf, linked_spawn)]
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
    pub current_slot: usize,
    pub trick_score: f32,
    pub stars: f32,
    pub completed_tricks: u32,
    pub failed_tricks: u32,
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
    pub score: f32,
    pub failed: bool,
}

#[derive(Message, Clone, Copy)]
pub struct ShowRunFinished {
    pub show: Entity,
    pub stage: Entity,
}

/// Live repetition history for tricks performed in this world.
#[derive(Component, Default)]
pub struct WorldTrickPopularity(pub HashMap<AssetId, f32>);

/// The guest's authored desire to attend a show, in source need units.
#[derive(Component)]
pub struct GuestShowNeed {
    pub value: f32,
    pub triggered: bool,
    pub cessation: f32,
    pub trigger: f32,
}

impl GuestShowNeed {
    pub fn from_authored(
        initial: f32,
        inclusive_range: Option<u32>,
        trigger: f32,
        cessation: f32,
        draw: impl FnOnce() -> u16,
    ) -> Self {
        let offset =
            inclusive_range.map_or(0, |extent| u32::from(draw()) % extent.saturating_add(1));
        let value = (initial + offset as f32).clamp(0.0, 100.0);
        Self {
            value,
            triggered: value >= trigger,
            cessation,
            trigger,
        }
    }

    pub fn adjust(&mut self, delta: f32) {
        self.value = (self.value + delta).clamp(0.0, 100.0);
        if self.value >= self.trigger {
            self.triggered = true;
        } else if self.value <= self.cessation {
            self.triggered = false;
        }
    }

    pub fn source_need(&self) -> (i32, bool) {
        ((self.value * 65_536.0).round() as i32, self.triggered)
    }
}

#[cfg(test)]
mod tests {
    use super::GuestShowNeed;

    #[test]
    fn authored_show_need_draws_whole_steps_and_clamps_adjustments() {
        let mut need = GuestShowNeed::from_authored(50.0, Some(25), 0.0, -100.0, || 25);
        assert_eq!(need.value, 75.0);
        need.adjust(50.0);
        assert_eq!(need.value, 100.0);
        need.adjust(-150.0);
        assert_eq!(need.value, 0.0);

        let fixed = GuestShowNeed::from_authored(11.25, None, 0.0, -100.0, || {
            panic!("fixed value requested a draw")
        });
        assert_eq!(fixed.value, 11.25);
    }
}
