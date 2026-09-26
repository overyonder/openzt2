use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::{
    simulation_clock_types::{ZooCalendar, ZooClock, ZooDayAdvanced, ZooYearAdvanced},
    zoo_calendar_calculation::calculate_zoo_calendar_for_absolute_day,
};

pub(in crate::plugins::simulation_time) fn advance_authoritative_zoo_clock_and_calendar(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut zoo_clock: ResMut<ZooClock>,
    mut zoo_calendar: ResMut<ZooCalendar>,
    mut advanced_days: MessageWriter<ZooDayAdvanced>,
    mut advanced_years: MessageWriter<ZooYearAdvanced>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let previous_absolute_day = zoo_clock.absolute_day;
    if !advance_zoo_clock_by_one_tick(&mut zoo_clock, definitions.timing().ticks_per_day) {
        return;
    }
    if zoo_clock.absolute_day == previous_absolute_day {
        return;
    }
    let previous_calendar = *zoo_calendar;
    *zoo_calendar = calculate_zoo_calendar_for_absolute_day(
        &definitions.timing().calendar,
        zoo_clock.absolute_day,
    );
    advanced_days.write(ZooDayAdvanced {
        previous_day: previous_absolute_day,
        current_day: zoo_clock.absolute_day,
        calendar: *zoo_calendar,
    });
    if zoo_calendar.year != previous_calendar.year {
        advanced_years.write(ZooYearAdvanced {
            previous_year: previous_calendar.year,
            current_year: zoo_calendar.year,
            calendar: *zoo_calendar,
        });
    }
}

pub(crate) fn advance_zoo_clock_by_one_tick(zoo_clock: &mut ZooClock, ticks_per_day: u32) -> bool {
    if ticks_per_day == 0 || zoo_clock.tick_in_day >= ticks_per_day {
        return false;
    }
    let Some(next_tick) = zoo_clock.tick.checked_add(1) else {
        return false;
    };
    zoo_clock.tick = next_tick;
    zoo_clock.tick_in_day += 1;
    if zoo_clock.tick_in_day == ticks_per_day {
        let Some(next_absolute_day) = zoo_clock.absolute_day.checked_add(1) else {
            zoo_clock.tick_in_day -= 1;
            zoo_clock.tick -= 1;
            return false;
        };
        zoo_clock.tick_in_day = 0;
        zoo_clock.absolute_day = next_absolute_day;
    }
    true
}
