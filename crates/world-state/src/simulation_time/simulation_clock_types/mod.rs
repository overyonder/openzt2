use bevy::prelude::*;

/// Zoo time in fixed simulation ticks.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZooClock {
    pub tick: u64,
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
