use bevy::prelude::*;
use openzt2_game_data::species::NeedKind;

pub(crate) const Q16_ONE: i32 = 1 << 16;
pub(crate) const MAX_NEED_Q16: i32 = 1000 * Q16_ONE;

macro_rules! need_component {
    ($name:ident) => {
        #[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub(crate) struct $name(pub i32);
    };
}

need_component!(Hunger);
need_component!(Thirst);
need_component!(RestNeed);
need_component!(PrivacyNeed);
need_component!(SocialNeed);
need_component!(ExerciseNeed);
need_component!(StimulationNeed);
need_component!(EnvironmentNeed);
need_component!(HealthNeed);
need_component!(HygieneNeed);
need_component!(BathroomNeed);

/// Hysteretic BFAI need-trigger facts. A bit sets and clears only at the
/// species-authored trigger and cessation thresholds.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NeedTriggerState(pub u16);

impl NeedTriggerState {
    pub(crate) fn observe(
        &mut self,
        index: usize,
        value_q16: i32,
        trigger: Option<u16>,
        cessation: Option<u16>,
    ) {
        let bit = 1_u16 << index;
        if trigger.is_some_and(|threshold| value_q16 <= i32::from(threshold) * Q16_ONE) {
            self.0 |= bit;
        } else if cessation.is_some_and(|threshold| value_q16 >= i32::from(threshold) * Q16_ONE) {
            self.0 &= !bit;
        }
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct HabitatSuitability {
    pub(crate) space: u16,
    pub(crate) biome: u16,
    pub(crate) overall: u16,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AnimalWelfare(pub u16);

/// Raw BFAI need-point totals accumulated over this animal's lifetime.
///
/// These are deliberately separate from the clamped current need-point
/// branches. Original branch changes clear the opposite current branch but do
/// not reset these totals; scenario rules take baselines instead.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct CumulativeNeedPoints {
    pub(crate) good: f32,
    pub(crate) bad: f32,
}

/// A focused outcome emitted by whichever behavior changed the active BFAI
/// need-point branch. Values are the raw deltas, before current-branch clamps.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub(crate) struct AccumulateNeedPoints {
    pub(crate) animal: Entity,
    pub(crate) good: f32,
    pub(crate) bad: f32,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum WelfareBand {
    Critical,
    Poor,
    #[default]
    Fair,
    Good,
    Excellent,
}

macro_rules! adjustment_message {
    ($name:ident) => {
        #[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
        pub(crate) struct $name {
            pub(crate) animal: Entity,
            pub(crate) delta_q16: i32,
        }
    };
}

adjustment_message!(AdjustHunger);
adjustment_message!(AdjustThirst);
adjustment_message!(AdjustRest);
adjustment_message!(AdjustPrivacy);
adjustment_message!(AdjustSocial);
adjustment_message!(AdjustExercise);
adjustment_message!(AdjustStimulation);
adjustment_message!(AdjustEnvironment);
adjustment_message!(AdjustHealthNeed);
adjustment_message!(AdjustHygiene);
adjustment_message!(AdjustBathroom);

/// Sets one canonical animal need to an authored Q16 value.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SetAnimalNeed {
    pub(crate) animal: Entity,
    pub(crate) need: NeedKind,
    pub(crate) value_q16: i32,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NeedChanged {
    pub(crate) animal: Entity,
    pub(crate) need: NeedKind,
    pub(crate) value: u16,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WelfareChanged {
    pub(crate) animal: Entity,
    pub(crate) old: WelfareBand,
    pub(crate) new: WelfareBand,
}
