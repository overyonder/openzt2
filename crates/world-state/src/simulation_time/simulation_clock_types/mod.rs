use bevy::prelude::*;
use num_integer::Integer;

/// Zoo time in fixed simulation ticks.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZooClock {
    pub tick: u64,
    pub fractional_tick: ZooClockFraction,
    pub absolute_day: u32,
    pub tick_in_day: u32,
}

/// The display calendar derived from [`ZooClock::absolute_day`].
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZooCalendar {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZooDayAdvanced {
    pub previous_day: u32,
    pub current_day: u32,
    pub calendar: ZooCalendar,
}

/// The zoo calendar crossed into a new year.
///
/// This is emitted after [`ZooDayAdvanced`] for the same fixed clock tick.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZooYearAdvanced {
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
