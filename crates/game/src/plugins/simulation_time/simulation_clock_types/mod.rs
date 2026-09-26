use bevy::prelude::*;

/// Zoo time in fixed simulation ticks.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ZooClock {
    pub(crate) tick: u64,
    pub(crate) absolute_day: u32,
    pub(crate) tick_in_day: u32,
}

/// The display calendar derived from [`ZooClock::absolute_day`].
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ZooCalendar {
    pub(crate) year: u16,
    pub(crate) month: u8,
    pub(crate) day: u8,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ZooDayAdvanced {
    pub(crate) previous_day: u32,
    pub(crate) current_day: u32,
    pub(crate) calendar: ZooCalendar,
}

/// The zoo calendar crossed into a new year.
///
/// This is emitted after [`ZooDayAdvanced`] for the same fixed clock tick.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ZooYearAdvanced {
    pub(crate) previous_year: u16,
    pub(crate) current_year: u16,
    pub(crate) calendar: ZooCalendar,
}
