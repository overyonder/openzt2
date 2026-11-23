use bevy::{ecs::query::QueryData, prelude::*};

#[derive(Component)]
pub(crate) struct NeedLatch<const NEED: usize>(pub(crate) bool);

#[derive(Bundle)]
pub struct AnimalNeedLatchBundle {
    hunger: NeedLatch<0>,
    thirst: NeedLatch<1>,
    rest: NeedLatch<2>,
    privacy: NeedLatch<3>,
    social: NeedLatch<4>,
    exercise: NeedLatch<5>,
    stimulation: NeedLatch<6>,
    environment: NeedLatch<7>,
    health: NeedLatch<8>,
    hygiene: NeedLatch<9>,
    bathroom: NeedLatch<10>,
    reproduction: NeedLatch<11>,
}

impl AnimalNeedLatchBundle {
    #[must_use]
    pub fn from_saved_mask(mask: u16) -> Self {
        Self {
            hunger: NeedLatch(mask & (1 << 0) != 0),
            thirst: NeedLatch(mask & (1 << 1) != 0),
            rest: NeedLatch(mask & (1 << 2) != 0),
            privacy: NeedLatch(mask & (1 << 3) != 0),
            social: NeedLatch(mask & (1 << 4) != 0),
            exercise: NeedLatch(mask & (1 << 5) != 0),
            stimulation: NeedLatch(mask & (1 << 6) != 0),
            environment: NeedLatch(mask & (1 << 7) != 0),
            health: NeedLatch(mask & (1 << 8) != 0),
            hygiene: NeedLatch(mask & (1 << 9) != 0),
            bathroom: NeedLatch(mask & (1 << 10) != 0),
            reproduction: NeedLatch(mask & (1 << 11) != 0),
        }
    }
}

#[derive(QueryData)]
pub struct AnimalNeedLatches {
    hunger: &'static NeedLatch<0>,
    thirst: &'static NeedLatch<1>,
    rest: &'static NeedLatch<2>,
    privacy: &'static NeedLatch<3>,
    social: &'static NeedLatch<4>,
    exercise: &'static NeedLatch<5>,
    stimulation: &'static NeedLatch<6>,
    environment: &'static NeedLatch<7>,
    health: &'static NeedLatch<8>,
    hygiene: &'static NeedLatch<9>,
    bathroom: &'static NeedLatch<10>,
    reproduction: &'static NeedLatch<11>,
}

impl AnimalNeedLatchesItem<'_, '_> {
    pub(crate) fn triggered(&self, rank: usize) -> bool {
        match rank {
            0 => self.hunger.0,
            1 => self.thirst.0,
            2 => self.rest.0,
            3 => self.privacy.0,
            4 => self.social.0,
            5 => self.exercise.0,
            6 => self.stimulation.0,
            7 => self.environment.0,
            8 => self.health.0,
            9 => self.hygiene.0,
            10 => self.bathroom.0,
            11 => self.reproduction.0,
            _ => false,
        }
    }

    pub(crate) fn any_triggered(&self) -> bool {
        self.hunger.0
            || self.thirst.0
            || self.rest.0
            || self.privacy.0
            || self.social.0
            || self.exercise.0
            || self.stimulation.0
            || self.environment.0
            || self.health.0
            || self.hygiene.0
            || self.bathroom.0
            || self.reproduction.0
    }

    #[must_use]
    pub fn saved_mask(&self) -> u16 {
        (0..12).fold(0, |mask, rank| {
            mask | (u16::from(self.triggered(rank)) << rank)
        })
    }
}
