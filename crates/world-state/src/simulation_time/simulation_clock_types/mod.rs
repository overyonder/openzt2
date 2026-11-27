use bevy::prelude::*;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ZooClockAdvance;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZooTimekeeperElapsed {
    pub root: Entity,
    pub event: openzt2_game_assets::world_definitions::timing::TimekeeperEvent,
    pub count: u32,
}
use num_integer::Integer;

/// Zoo time in fixed simulation ticks.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZooClock {
    pub tick: u64,
    pub fractional_tick: ZooClockFraction,
    pub absolute_day: u32,
    pub tick_in_day: u32,
}

/// Absolute zoo day, with change detection independent of the fixed tick.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ZooDay(pub u32);

/// The display calendar derived from [`ZooClock::absolute_day`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZooCalendar {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

/// Calendar month ordinal, updated only when the zoo enters a new month.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZooMonth(pub u32);

impl From<ZooCalendar> for ZooMonth {
    fn from(calendar: ZooCalendar) -> Self {
        Self(u32::from(calendar.year) * 12 + u32::from(calendar.month.saturating_sub(1)))
    }
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZooDayAdvanced {
    pub root: Entity,
    pub previous_day: u32,
    pub current_day: u32,
    pub completed_month: Option<ZooMonth>,
    pub calendar: ZooCalendar,
}

/// The zoo calendar crossed into a new year.
///
/// This is emitted after [`ZooDayAdvanced`] for the same fixed clock tick.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZooYearAdvanced {
    pub root: Entity,
    pub previous_year: u16,
    pub current_year: u16,
    pub calendar: ZooCalendar,
}

/// Exact position within the current fixed tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZooClockFraction {
    numerator: u64,
    denominator: u64,
}

impl Default for ZooClockFraction {
    fn default() -> Self {
        Self {
            numerator: 0,
            denominator: 1,
        }
    }
}

impl ZooClockFraction {
    pub fn from_ratio(numerator: u64, denominator: u64) -> Option<Self> {
        if denominator == 0 || numerator >= denominator {
            return None;
        }
        let divisor = numerator.gcd(&denominator);
        Some(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    pub fn numerator(self) -> u64 {
        self.numerator
    }
    pub fn denominator(self) -> u64 {
        self.denominator
    }
}

/// Scaled zoo nanoseconds per fixed tick, retained as a reduced ratio.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZooDayCadence {
    tick_numerator: u128,
    tick_denominator: u128,
    day_numerator: u128,
}

impl ZooDayCadence {
    const ZOO_DAY_NANOSECONDS: u128 = 86_400 * 1_000_000_000;

    pub fn new(simulation_step: std::time::Duration, scale: impl Into<f64>) -> Option<Self> {
        let scale = scale.into();
        if !scale.is_normal() || scale <= 0.0 || simulation_step.is_zero() {
            return None;
        }
        // A parsed scale is a binary fraction. Keep it exact so even the
        // sub-nanosecond part of each scaled step carries into the next day.
        let bits = scale.to_bits();
        let mantissa_bits = f64::MANTISSA_DIGITS - 1;
        let leading_bit = 1_u64 << mantissa_bits;
        let mantissa = (bits & (leading_bit - 1)) | leading_bit;
        let exponent = i32::try_from((bits >> mantissa_bits) & 0x7ff).ok()?
            - 1023
            - i32::try_from(mantissa_bits).ok()?;
        let step = simulation_step
            .as_nanos()
            .checked_mul(u128::from(mantissa))?;
        let (tick_numerator, tick_denominator) = if exponent >= 0 {
            (
                step.checked_mul(1_u128.checked_shl(exponent.unsigned_abs())?)?,
                1,
            )
        } else {
            let shift = exponent.unsigned_abs();
            let common = step.trailing_zeros().min(shift);
            (step >> common, 1_u128.checked_shl(shift - common)?)
        };
        let day_numerator = Self::ZOO_DAY_NANOSECONDS.checked_mul(tick_denominator)?;
        u32::try_from(day_numerator / tick_numerator).ok()?;
        Some(Self {
            tick_numerator,
            tick_denominator,
            day_numerator,
        })
    }

    pub fn from_ticks_per_day(ticks_per_day: std::num::NonZeroU32) -> Self {
        let ticks = u128::from(ticks_per_day.get());
        let common = Self::ZOO_DAY_NANOSECONDS.gcd(&ticks);
        Self {
            tick_numerator: Self::ZOO_DAY_NANOSECONDS / common,
            tick_denominator: ticks / common,
            day_numerator: Self::ZOO_DAY_NANOSECONDS * (ticks / common),
        }
    }

    pub fn clock_at(self, tick: u64, fractional_tick: ZooClockFraction) -> Option<ZooClock> {
        let elapsed = self.elapsed_numerator(tick, fractional_tick)?;
        let absolute_day = u32::try_from(elapsed / self.day_numerator).ok()?;
        let tick_in_day =
            u32::try_from((elapsed % self.day_numerator) / self.tick_numerator).ok()?;
        Some(ZooClock {
            tick,
            fractional_tick,
            absolute_day,
            tick_in_day,
        })
    }

    pub fn clock_at_elapsed_zoo_time(self, elapsed: std::time::Duration) -> Option<ZooClock> {
        let elapsed = elapsed.as_nanos().checked_mul(self.tick_denominator)?;
        let tick = u64::try_from(elapsed / self.tick_numerator).ok()?;
        let remainder = elapsed % self.tick_numerator;
        let divisor = remainder.gcd(&self.tick_numerator);
        let fraction = ZooClockFraction::from_ratio(
            u64::try_from(remainder / divisor).ok()?,
            u64::try_from(self.tick_numerator / divisor).ok()?,
        )?;
        self.clock_at(tick, fraction)
    }

    /// The fractional tick count in one authored zoo day.
    pub fn ticks_per_zoo_day(self) -> f64 {
        self.day_numerator as f64 / self.tick_numerator as f64
    }

    pub fn day_fraction(self, clock: ZooClock) -> Option<f64> {
        let elapsed = self.elapsed_numerator(clock.tick, clock.fractional_tick)?;
        Some((elapsed % self.day_numerator) as f64 / self.day_numerator as f64)
    }

    pub fn next_clock(self, clock: ZooClock) -> Option<ZooClock> {
        let tick = clock.tick.checked_add(1)?;
        let elapsed = self.elapsed_numerator(clock.tick, clock.fractional_tick)?;
        let advanced = (elapsed % self.day_numerator).checked_add(self.tick_numerator)?;
        let crossed_days = u32::try_from(advanced / self.day_numerator).ok()?;
        let tick_in_day =
            u32::try_from(advanced % self.day_numerator / self.tick_numerator).ok()?;
        Some(ZooClock {
            tick,
            fractional_tick: clock.fractional_tick,
            absolute_day: clock.absolute_day.checked_add(crossed_days)?,
            tick_in_day,
        })
    }

    fn elapsed_numerator(self, tick: u64, fractional_tick: ZooClockFraction) -> Option<u128> {
        let numerator = u128::from(fractional_tick.numerator());
        let denominator = u128::from(fractional_tick.denominator());
        let scaled_fraction = (self.tick_numerator / denominator) * numerator
            + (self.tick_numerator % denominator) * numerator / denominator;
        u128::from(tick)
            .checked_mul(self.tick_numerator)?
            .checked_add(scaled_fraction)
    }
}

/// An authored text node showing the localized zoo date.
#[derive(Component)]
pub struct UiZooDateText;
