use bevy::prelude::*;
use openzt2_game_data::species::NeedKind;

pub const Q16_ONE: i32 = 1 << 16;
pub const MAX_NEED_Q16: i32 = 1000 * Q16_ONE;

#[derive(Component, Debug, Clone, Copy)]
pub struct AnimalNeedAdjustmentTimer {
    pub remaining_ns: u64,
    pub random: openzt2_world_state::simulation_time::deterministic_random_stream::DeterministicRng,
}

macro_rules! need_component {
    ($name:ident) => {
        #[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
        pub struct $name(pub i32);
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
// Outside the welfare score; its trigger is `REPRODUCTION_TRIGGER_INDEX`.
need_component!(ReproductionNeed);

/// The `NeedTriggerState` bit of the reproduction need, after the eleven
/// welfare needs.
pub const REPRODUCTION_TRIGGER_INDEX: usize = 11;

/// Hysteretic BFAI need-trigger facts. A bit sets and clears only at the
/// species-authored trigger and cessation thresholds.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NeedTriggerState(pub u16);

impl NeedTriggerState {
    pub fn observe(
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
pub struct HabitatSuitability {
    pub space: u16,
    pub biome: u16,
    pub overall: u16,
}

/// The habitat score has been applied as the environment need's baseline.
#[derive(Component, Debug, Clone, Copy)]
pub struct EnvironmentHabitatAssessed;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AnimalWelfare(pub u16);

/// Raw BFAI need-point totals accumulated over this animal's lifetime.
///
/// These are deliberately separate from the clamped current need-point
/// branches. Original branch changes clear the opposite current branch but do
/// not reset these totals; scenario rules take baselines instead.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct CumulativeNeedPoints {
    pub good: f32,
    pub bad: f32,
}

/// A focused outcome emitted by whichever behavior changed the active BFAI
/// need-point branch. Values are the raw deltas, before current-branch clamps.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct AccumulateNeedPoints {
    pub animal: Entity,
    pub good: f32,
    pub bad: f32,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WelfareBand {
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
        pub struct $name {
            pub animal: Entity,
            pub delta_q16: i32,
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
adjustment_message!(AdjustReproduction);

/// Sets one canonical animal need to an authored Q16 value.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetAnimalNeed {
    pub animal: Entity,
    pub need: NeedKind,
    pub value_q16: i32,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct NeedChanged {
    pub animal: Entity,
    pub need: NeedKind,
    pub value: u16,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WelfareChanged {
    pub animal: Entity,
    pub old: WelfareBand,
    pub new: WelfareBand,
}
