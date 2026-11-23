use bevy::prelude::*;
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
